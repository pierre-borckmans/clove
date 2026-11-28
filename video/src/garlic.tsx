// The desktop app's map, redrawn for the video: the golden image as a garlic
// bulb in line art, clones as cloves on stems around it, checkpoints as skins,
// forks growing off their parent. Mirrors apps/desktop's map (bulb.tsx,
// nodes.tsx, styles.css), without importing it.
import type { CSSProperties, ReactNode } from "react";
import { interpolate, useCurrentFrame } from "remotion";
import { c, mono, serif } from "./ui";

/** The map's palette, dark theme (styles.css .dark). */
export const g = {
  bg: "#141816",
  pane: "#101412",
  line: "rgba(255,255,255,0.07)",
  text: "#e8ebe7",
  muted: "#8f9792",
  faint: "#5f6762",
  bulbFill: "color-mix(in oklab, #ddd3a6 9%, #141816)",
  bulbInk: "color-mix(in oklab, #ddd3a6 75%, transparent)",
  stem: "color-mix(in oklab, #7cc49b 30%, rgba(255,255,255,0.16))",
  stemOld: "rgba(232,235,231,0.24)",
  amber: "oklch(0.78 0.12 75)",
  rose: "oklch(0.72 0.11 18)",
  sage: "#7cc49b",
};

export type CloveState = "clean" | "modified" | "attention" | "merged" | "unmounted";

/** Fill and stroke per skin state (styles.css .map-skin-*). */
export const SKIN: Record<CloveState, { fill: string; stroke: string }> = {
  clean: { fill: "#ddd3a6", stroke: "#e3c95e" },
  modified: { fill: "oklch(0.72 0.15 68)", stroke: "oklch(0.87 0.13 75)" },
  attention: { fill: "oklch(0.66 0.12 18)", stroke: "oklch(0.72 0.11 18)" },
  merged: { fill: "oklch(0.7 0.13 150)", stroke: "oklch(0.86 0.1 140)" },
  unmounted: { fill: "color-mix(in oklab, oklch(0.255 0.008 85) 70%, transparent)", stroke: "oklch(0.56 0.03 85)" },
};

export const CLOVE_PATH = "M50 8 C62 28 74 52 70 76 C68 90 32 90 30 76 C26 52 38 28 50 8Z";
const C_PATH = "M73.04 29.26 A31 31 0 1 0 73.04 70.74";

const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
export const ramp = (f: number, a: number, b: number) => interpolate(f, [a, b], [0, 1], clamp);
export const ease = (t: number) => t * t * (3 - 2 * t);
export const easeOut = (t: number) => 1 - (1 - t) ** 3;

/* ------------------------------------------------------------------ bulb */

const BULB_R = 72;
const LOBES = 9;
const PLATE_R = 15;

function seeded(seed: number) {
  let a = (seed * 2654435761) >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function bulbShape(seed: number) {
  const rnd = seeded(seed);
  const turn = rnd() * Math.PI * 2;
  const amp = Array.from({ length: LOBES }, () => 0.8 + rnd() * 0.4);
  const width = (Math.PI * 2) / LOBES;
  const rim = (theta: number) => {
    const t = (((theta - turn) % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
    const k = Math.floor(t / width);
    const u = t / width - k;
    return BULB_R * (0.86 + 0.14 * Math.sin(Math.PI * u) ** 0.65 * amp[k]);
  };
  return { turn, width, rim };
}

/** A generation's contour radius at `theta`, in bulb units: age 0 is the rim. */
const contourR = (shape: ReturnType<typeof bulbShape>, age: number, theta: number) =>
  PLATE_R + (shape.rim(theta) - PLATE_R) * (1 - Math.min(age, 3) * 0.2);

function closed(r: (a: number) => number, k: number, n = 216) {
  const pts = Array.from({ length: n }, (_, i) => {
    const a = (i / n) * Math.PI * 2;
    return `${(Math.cos(a) * r(a) * k).toFixed(2)} ${(Math.sin(a) * r(a) * k).toFixed(2)}`;
  });
  return `M ${pts.join(" L ")} Z`;
}

function skinLines(shape: ReturnType<typeof bulbShape>, seed: number, k: number) {
  const rnd = seeded(seed * 31 + 7);
  const out: string[] = [];
  const p = (a: number, r: number) => `${(Math.cos(a) * r * k).toFixed(2)} ${(Math.sin(a) * r * k).toFixed(2)}`;
  for (let i = 0; i < LOBES; i++) {
    for (const u of [0, 0.3 + rnd() * 0.4]) {
      const a = shape.turn + (i + u) * shape.width;
      const mid = shape.turn + (i + 0.5) * shape.width;
      const r1 = shape.rim(a) - 1.2;
      const r0 = r1 * (0.6 + rnd() * 0.08);
      const am = a + (u === 0 ? 0 : (mid - a) * 0.15);
      out.push(`M ${p(a, r0)} Q ${p(am, (r0 + r1) / 2)} ${p(a, r1)}`);
    }
  }
  return out.join(" ");
}

/** Where a clove of generation `age` roots into the bulb at angle `theta` (radians), relative to its centre. */
export function rootPoint(seed: number, r: number, age: number, theta: number) {
  const shape = bulbShape(seed);
  const rr = (contourR(shape, age, theta) * r) / BULB_R;
  return { x: Math.cos(theta) * rr, y: Math.sin(theta) * rr };
}

/** The golden image as a bulb, centred on x,y with rim radius `r`. */
export function Bulb({
  x,
  y,
  r,
  seed = 14,
  deepest = 3,
  name,
  sub,
  appear = 1,
  lit,
  breath = 0,
  labelSize = 18,
}: {
  x: number;
  y: number;
  r: number;
  seed?: number;
  /** Older generations drawn inside, as dashed contours. */
  deepest?: number;
  name?: string;
  sub?: string;
  appear?: number;
  /** A generation lit green (selected or hovered). */
  lit?: number;
  /** A refresh: the drawing breathes and its ink turns green. */
  breath?: number;
  labelSize?: number;
}) {
  const shape = bulbShape(seed);
  const k = r / BULB_R;
  const s = (0.86 + 0.14 * appear) * (1 + breath * 0.03);
  const ink = breath > 0 ? `color-mix(in oklab, ${c.bottle} ${Math.round(breath * 50)}%, ${g.bulbInk})` : g.bulbInk;
  const innermost = Math.min(...Array.from({ length: 72 }, (_, i) => contourR(shape, deepest, (i / 72) * Math.PI * 2))) * k;
  const mark = Math.min(38 * k, 2 * innermost - 12 * k);
  return (
    <g transform={`translate(${x} ${y})`} opacity={appear}>
      <g transform={`scale(${s})`}>
        <path d={closed(shape.rim, k)} fill={g.bulbFill} stroke={ink} strokeWidth={1.4 * Math.max(1, k * 0.8)} strokeLinejoin="round" />
        <path d={skinLines(shape, seed, k)} fill="none" stroke={ink} strokeWidth={0.7 * Math.max(1, k * 0.8)} strokeLinecap="round" opacity={0.3} />
        {Array.from({ length: deepest }, (_, i) => i + 1).map((age) => (
          <path
            key={age}
            d={closed((a) => contourR(shape, age, a), k)}
            fill="none"
            stroke={lit === age ? c.bottle : ink}
            strokeWidth={(lit === age ? 2 : 1.5) * Math.max(1, k * 0.7)}
            strokeDasharray={`${4 * k} ${3 * k}`}
            opacity={lit === age ? 1 : 0.7}
          />
        ))}
        {lit === 0 && <path d={closed(shape.rim, k)} fill="none" stroke={c.bottle} strokeWidth={2 * Math.max(1, k * 0.8)} />}
        <g transform={`scale(${mark / 100}) translate(-50 -50)`} opacity={0.3}>
          <path d={C_PATH} fill="none" stroke={ink} strokeWidth={19} strokeLinecap="round" />
          <path d={CLOVE_PATH} fill={ink} transform="translate(53 51) rotate(16) scale(0.33) translate(-50 -49)" />
        </g>
      </g>
      {name && (
        <g fontFamily={serif} textAnchor="middle" style={{ paintOrder: "stroke" }}>
          <text y={r + labelSize * 1.6} fontSize={labelSize * 1.08} fontWeight={500} fill={g.text} stroke={g.bg} strokeWidth={5}>
            {name}
          </text>
          {sub && (
            <text y={r + labelSize * 2.85} fontSize={labelSize * 0.84} fontFamily={mono} fill={g.muted} stroke={g.bg} strokeWidth={5}>
              {sub}
            </text>
          )}
        </g>
      )}
    </g>
  );
}

/* ----------------------------------------------------------------- cloves */

/** One clove, centred on x,y, its tip pointing at `angle` degrees (0 is up). */
export function Clove({
  x,
  y,
  angle,
  length,
  state = "clean",
  tint,
  skins = 0,
  skinGlow = 0,
  age = 0,
  selected = false,
  grow = 1,
  opacity = 1,
  style,
}: {
  x: number;
  y: number;
  angle: number;
  length: number;
  state?: CloveState;
  /** Overrides the state's colours, for blends between two states. */
  tint?: { fill: string; stroke: string };
  /** Checkpoints, drawn as thin skins round the clove; a fraction is one forming. */
  skins?: number;
  /** Lights the skins green for a moment (a rewind). */
  skinGlow?: number;
  /** An older image: paler. */
  age?: number;
  selected?: boolean;
  grow?: number;
  opacity?: number;
  style?: CSSProperties;
}) {
  const col = tint ?? SKIN[state];
  const paper = "oklch(0.58 0.008 90)";
  const fill = age ? `color-mix(in oklab, ${col.fill} ${100 - age * 24}%, ${paper})` : col.fill;
  const stroke = age ? `color-mix(in oklab, ${col.stroke} ${100 - age * 26}%, ${paper})` : col.stroke;
  const k = length / 88;
  const w = (length * 40) / 88;
  const h = (length * 82) / 88;
  const layers = Math.ceil(skins);
  return (
    <g transform={`translate(${x} ${y}) rotate(${angle}) scale(${grow})`} opacity={opacity} style={style}>
      {Array.from({ length: Math.min(layers, 4) }, (_, i) => {
        const forming = i === layers - 1 ? skins - Math.floor(skins) || 1 : 1;
        const d = (2.8 + i * 2.2) * (length / 30) * (0.4 + 0.6 * easeOut(forming));
        const sx = (w + 2 * d) / w;
        const sy = (h + 2 * d) / h;
        const skinStroke = skinGlow > 0 ? `color-mix(in oklab, ${c.bottle} ${Math.round(skinGlow * 100)}%, #e3c95e)` : `color-mix(in oklab, #e3c95e ${70 - i * 18}%, ${g.muted})`;
        return (
          <g key={i} transform={`scale(${sx} ${sy})`} opacity={(0.85 - i * 0.2) * forming}>
            <path d={CLOVE_PATH} transform={`scale(${k}) translate(-50 -48)`} fill="none" stroke={skinStroke} strokeWidth={1.7 + skinGlow * 1.5} vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
          </g>
        );
      })}
      {selected && (
        <path d={CLOVE_PATH} transform={`scale(${k}) translate(-50 -48)`} fill="none" stroke={c.bottle} strokeWidth={14} opacity={0.25} style={{ filter: `blur(${length / 14}px)` }} />
      )}
      <g transform={`scale(${k}) translate(-50 -48)`}>
        <path
          d={CLOVE_PATH}
          fill={fill}
          stroke={selected ? c.bottle : stroke}
          strokeWidth={selected ? 6 : 4}
          strokeLinejoin="round"
          strokeDasharray={state === "unmounted" ? "7 5" : undefined}
        />
        <path d="M50 24 C46 44 46 64 50 80" fill="none" stroke={stroke} strokeWidth={2} opacity={0.6} strokeDasharray={age ? "3 2" : undefined} />
      </g>
    </g>
  );
}

/** Where a clove's base sits, given its centre, tip angle (0 up) and length. */
export function baseOf(x: number, y: number, angle: number, length: number) {
  const a = (angle * Math.PI) / 180;
  return { x: x - Math.sin(a) * length * 0.45, y: y + Math.cos(a) * length * 0.45 };
}

/** A stem from (x0,y0) to (x1,y1), bowing by `bend`, drawn up to `draw` (0 to 1). */
export function Stem({
  x0,
  y0,
  x1,
  y1,
  bend = 0.12,
  draw = 1,
  old = false,
  strain = 0,
  width = 1.6,
  opacity = 1,
  lit = false,
}: {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  bend?: number;
  draw?: number;
  old?: boolean;
  /** A pull: fibres separate (dashes open) and the stem reddens. */
  strain?: number;
  width?: number;
  opacity?: number;
  lit?: boolean;
}) {
  const mx = (x0 + x1) / 2 - (y1 - y0) * bend;
  const my = (y0 + y1) / 2 + (x1 - x0) * bend;
  const d = `M ${x0} ${y0} Q ${mx} ${my} ${x1} ${y1}`;
  const stroke = strain > 0 ? `color-mix(in oklch, ${g.rose} ${Math.round(strain * 100)}%, ${c.bottle})` : lit ? c.bottle : old ? g.stemOld : g.stem;
  const dash = strain > 0 ? `${(6 - strain * 2) * width} ${(1 + strain * 3) * width}` : old ? `${3 * width} ${4 * width}` : undefined;
  if (draw <= 0) return null;
  return (
    <g opacity={opacity}>
      {draw < 1 || dash === undefined ? (
        <path d={d} fill="none" stroke={stroke} strokeWidth={width + strain * 0.6} strokeLinecap="round" pathLength={1} strokeDasharray={`${draw} 1`} />
      ) : null}
      {draw >= 1 && dash !== undefined && <path d={d} fill="none" stroke={stroke} strokeWidth={width + strain * 0.6} strokeLinecap="round" strokeDasharray={dash} />}
    </g>
  );
}

/** A name and its small state line, as on the map. */
export function Label({
  x,
  y,
  name,
  parts = [],
  align = "start",
  size = 17,
  opacity = 1,
  selected = false,
}: {
  x: number;
  y: number;
  name: string;
  parts?: [string, string?][];
  align?: "start" | "middle" | "end";
  size?: number;
  opacity?: number;
  selected?: boolean;
}) {
  const left = align === "start" ? x : align === "end" ? x - 400 : x - 200;
  return (
    <foreignObject x={left} y={y - size * 0.9} width={400} height={size * 3} opacity={opacity} style={{ overflow: "visible" }}>
      <div style={{ fontFamily: serif, textAlign: align === "start" ? "left" : align === "end" ? "right" : "center", lineHeight: 1.25 }}>
        <div style={{ fontSize: size, fontWeight: 500, color: selected ? c.bottle : g.text, whiteSpace: "nowrap" }}>{name}</div>
        <div style={{ fontSize: size * 0.86, color: g.muted, whiteSpace: "nowrap", display: "flex", gap: size * 0.55, justifyContent: align === "start" ? "flex-start" : align === "end" ? "flex-end" : "center" }}>
          {parts.map(([t, col]) => (
            <span key={t} style={{ color: col ?? g.muted }}>
              {t}
            </span>
          ))}
        </div>
      </div>
    </foreignObject>
  );
}

/** The "+N" on a fold of forks. */
export function Badge({ x, y, n, size = 16, appear = 1 }: { x: number; y: number; n: number; size?: number; appear?: number }) {
  const w = size * 2.4;
  const h = size * 1.5;
  return (
    <g transform={`translate(${x} ${y}) scale(${0.6 + 0.4 * appear})`} opacity={appear}>
      <rect x={-w / 2} y={-h / 2} width={w} height={h} rx={h / 2} fill={g.bg} stroke="color-mix(in oklab, #7cc49b 55%, transparent)" strokeWidth={1.2} />
      <text y={size * 0.36} textAnchor="middle" fontFamily={serif} fontSize={size} fontWeight={600} fill={c.bottle}>
        +{n}
      </text>
    </g>
  );
}

/** A macOS arrow cursor, its tip on x,y. */
export function Cursor({ x, y, opacity = 1, down = false }: { x: number; y: number; opacity?: number; down?: boolean }) {
  return (
    <g transform={`translate(${x} ${y}) scale(${down ? 1.25 : 1.4})`} opacity={opacity}>
      <path d="M0 0 L0 17 L4.2 13.2 L7 19.5 L9.6 18.4 L6.9 12.3 L12.4 12.3 Z" fill="#fff" stroke="#000" strokeWidth={1.1} strokeLinejoin="round" />
    </g>
  );
}

/* -------------------------------------------------------------- the app */

/** Op progress, as the app stacks it: newest at the bottom, each sliding in. */
export type Toast = { at: number; until?: number; title: string; sub?: string; tone?: string; progress?: [number, number]; action?: string };

export function Toasts({ items, style, scale = 1 }: { items: Toast[]; style?: CSSProperties; scale?: number }) {
  const frame = useCurrentFrame();
  const live = items.filter((t) => frame >= t.at && (t.until === undefined || frame < t.until + 10));
  return (
    <div style={{ position: "absolute", display: "flex", flexDirection: "column", gap: 10 * scale, alignItems: "flex-end", ...style }}>
      {live.map((t) => {
        const p = easeOut(ramp(frame, t.at, t.at + 10));
        const out = t.until === undefined ? 0 : ramp(frame, t.until, t.until + 10);
        const prog = t.progress ? ramp(frame, t.progress[0], t.progress[1]) : null;
        return (
          <div
            key={t.title + t.at}
            style={{
              width: 360 * scale,
              padding: `${12 * scale}px ${16 * scale}px`,
              borderRadius: 12 * scale,
              background: "rgba(28,33,30,0.96)",
              border: `1px solid ${g.line}`,
              boxShadow: "0 18px 50px rgba(0,0,0,0.5)",
              fontFamily: serif,
              opacity: p * (1 - out),
              transform: `translateX(${(1 - p) * 40 * scale}px)`,
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 16 * scale, color: g.text }}>
              <span style={{ display: "flex", alignItems: "center", gap: 9 * scale }}>
                <span style={{ width: 8 * scale, height: 8 * scale, borderRadius: 8, background: t.tone ?? c.bottle }} />
                {t.title}
              </span>
              {t.action && <span style={{ fontSize: 14 * scale, color: c.bottle, fontWeight: 500 }}>{t.action}</span>}
            </div>
            {t.sub && <div style={{ fontFamily: mono, fontSize: 12 * scale, color: g.muted, marginTop: 5 * scale, whiteSpace: "nowrap" }}>{t.sub}</div>}
            {prog !== null && (
              <div style={{ height: 3 * scale, borderRadius: 3, background: g.line, marginTop: 9 * scale, overflow: "hidden" }}>
                <div style={{ width: `${prog * 100}%`, height: "100%", background: t.tone ?? c.bottle }} />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function NavItem({ label, right, rightColor, active = false }: { label: string; right?: string; rightColor?: string; active?: boolean }) {
  return (
    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "7px 10px", borderRadius: 8, background: active ? "rgba(255,255,255,0.06)" : undefined, fontSize: 15, color: g.text }}>
      <span>{label}</span>
      {right && <span style={{ fontFamily: mono, fontSize: 11, color: rightColor ?? g.muted }}>{right}</span>}
    </div>
  );
}

function Repo({ letter, name, n, dot, color, active = false }: { letter: string; name: string; n?: number; dot: string; color: string; active?: boolean }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "6px 10px", borderRadius: 8, background: active ? "rgba(255,255,255,0.06)" : undefined, fontSize: 15, color: g.text }}>
      <span style={{ width: 18, height: 18, borderRadius: 5, background: color, fontSize: 11, display: "flex", alignItems: "center", justifyContent: "center", color: "#fff" }}>{letter}</span>
      <span style={{ flex: 1 }}>{name}</span>
      {n !== undefined && <span style={{ fontSize: 12, color: g.faint }}>{n}</span>}
      <span style={{ width: 6, height: 6, borderRadius: 6, background: dot }} />
    </div>
  );
}

const LEGEND: [string, CloveState][] = [
  ["mounted", "clean"],
  ["unmounted", "unmounted"],
  ["modified", "modified"],
  ["needs attention", "attention"],
  ["merged", "merged"],
];

/**
 * The desktop app's window: title bar, sidebar, the repo's header with the
 * map, its legend, and a bottom panel that slides up (`panel`, 0 to 1).
 * `children` is an SVG drawn in the map's own coordinates (its top-left is 0,0).
 */
export function AppFrame({
  width,
  height,
  children,
  overlay,
  panel = 0,
  panelContent,
}: {
  width: number;
  height: number;
  children: ReactNode;
  overlay?: ReactNode;
  panel?: number;
  panelContent?: ReactNode;
}) {
  const side = 230;
  const mapW = width - side;
  const mapH = height - 44 - 54 - 36;
  const panelH = 190;
  return (
    <div style={{ width, height, borderRadius: 16, overflow: "hidden", border: `1px solid ${c.rule}`, boxShadow: "0 60px 160px rgba(0,0,0,0.65)", background: g.bg, fontFamily: serif, color: g.text, position: "relative" }}>
      <div style={{ height: 44, display: "flex", alignItems: "center", gap: 9, padding: "0 16px", borderBottom: `1px solid ${g.line}`, background: g.pane }}>
        {["#ec6a5e", "#f4bf4f", "#61c554"].map((bg) => (
          <div key={bg} style={{ width: 12, height: 12, borderRadius: 6, background: bg, opacity: 0.85 }} />
        ))}
        <span style={{ marginLeft: 70, fontSize: 14, color: g.text }}>clove</span>
        <span style={{ fontSize: 14, color: g.faint }}>›</span>
        <span style={{ fontSize: 14, color: g.muted }}>mono</span>
        <div style={{ marginLeft: 60, width: 330, height: 26, borderRadius: 7, border: `1px solid ${g.line}`, fontSize: 13, color: g.faint, display: "flex", alignItems: "center", padding: "0 10px" }}>Search or run a command</div>
        <span style={{ marginLeft: "auto", fontSize: 13, color: g.muted, display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ width: 6, height: 6, borderRadius: 6, background: c.bottle }} />
          service on
        </span>
      </div>
      <div style={{ display: "flex", height: height - 44 }}>
        <div style={{ width: side, padding: "16px 10px", display: "flex", flexDirection: "column", background: g.pane, borderRight: `1px solid ${g.line}` }}>
          <div style={{ fontSize: 12, color: g.muted, padding: "0 10px 8px" }}>Repositories</div>
          <Repo letter="C" name="clove" n={2} dot={c.bottle} color="#5b77a8" />
          <Repo letter="M" name="mono" n={9} dot={g.amber} color="#5b77a8" active />
          <Repo letter="S" name="site" dot={c.bottle} color="#9b7556" />
          <div style={{ flex: 1 }} />
          <NavItem label="Settings" right="● on" rightColor={c.bottle} />
          <NavItem label="Doctor" />
          <NavItem label="Cleanup" right="1 merged" rightColor={c.bottle} />
          <NavItem label="History" right="1 pruned" rightColor={g.amber} />
          <NavItem label="Activity" />
        </div>
        <div style={{ flex: 1, display: "flex", flexDirection: "column", position: "relative" }}>
          <div style={{ height: 54, display: "flex", alignItems: "center", gap: 12, padding: "0 18px", borderBottom: `1px solid ${g.line}` }}>
            <span style={{ width: 20, height: 20, borderRadius: 5, background: "#5b77a8", fontSize: 12, display: "flex", alignItems: "center", justifyContent: "center", color: "#fff" }}>M</span>
            <span style={{ fontSize: 17, fontWeight: 500 }}>mono</span>
            <span style={{ fontFamily: mono, fontSize: 13, color: g.muted }}>~/src/mono</span>
            <span style={{ marginLeft: "auto", display: "flex", border: `1px solid ${g.line}`, borderRadius: 8, fontSize: 13 }}>
              <span style={{ padding: "4px 10px", color: g.muted }}>List</span>
              <span style={{ padding: "4px 10px", color: c.bottle, background: "rgba(124,196,155,0.12)", borderRadius: 7 }}>Map</span>
            </span>
            <span style={{ fontSize: 14, color: "#fff", background: "#2f6b4c", borderRadius: 8, padding: "5px 12px" }}>+ New clone</span>
          </div>
          <div style={{ position: "relative", flex: 1, overflow: "hidden", backgroundImage: "radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)", backgroundSize: "22px 22px" }}>
            <svg width={mapW} height={mapH} style={{ position: "absolute", inset: 0, overflow: "visible" }}>
              {children}
            </svg>
            {overlay}
            <div
              style={{
                position: "absolute",
                left: 0,
                right: 0,
                bottom: 0,
                height: panelH,
                transform: `translateY(${(1 - panel) * (panelH + 10)}px)`,
                background: g.pane,
                borderTop: `1px solid ${g.line}`,
                boxShadow: "0 -20px 50px rgba(0,0,0,0.35)",
                padding: "16px 22px",
              }}
            >
              {panelContent}
            </div>
          </div>
          <div style={{ height: 36, display: "flex", alignItems: "center", gap: 18, padding: "0 18px", borderTop: `1px solid ${g.line}`, fontSize: 12.5, color: g.muted, background: g.bg, position: "relative" }}>
            {LEGEND.map(([l, s]) => (
              <span key={l} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <svg width={10} height={16} viewBox="24 4 52 88">
                  <path d={CLOVE_PATH} fill={SKIN[s].fill} stroke={SKIN[s].stroke} strokeWidth={5} strokeDasharray={s === "unmounted" ? "7 5" : undefined} />
                </svg>
                {l}
              </span>
            ))}
            <span>- - - older image</span>
            <span style={{ marginLeft: "auto", color: g.faint }}>Drag to pan · pinch to zoom</span>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------- the ring */

export type Spot = {
  name: string;
  /** Degrees, clockwise from the right (screen space). */
  deg: number;
  state: CloveState;
  parts?: [string, string?][];
  /** Generations behind the current image. */
  age?: number;
  skins?: number;
  /** The frame it sprouts at; undefined: already there. */
  at?: number;
  /** Ring radius, if not the default. */
  ring?: number;
  length?: number;
};

/** Where a ring clove sits, its tip angle, and where its stem starts and ends. */
export function place(s: Spot, bx: number, by: number, r: number, ring: number, length: number) {
  const th = (s.deg * Math.PI) / 180;
  const R = s.ring ?? ring;
  const x = bx + Math.cos(th) * R;
  const y = by + Math.sin(th) * R;
  const angle = s.deg + 90;
  const root = rootPoint(14, r, s.age ?? 0, th);
  const base = baseOf(x, y, angle, s.length ?? length);
  return { x, y, angle, th, x0: bx + root.x, y0: by + root.y, x1: base.x, y1: base.y };
}

/** Clones on one ring round the bulb, each sprouting on its stem from `at`. */
export function Ring({
  spots,
  bx,
  by,
  r,
  ring,
  length,
  labelSize = 16,
  dim,
  selected,
}: {
  spots: Spot[];
  bx: number;
  by: number;
  r: number;
  ring: number;
  length: number;
  labelSize?: number;
  /** Names drawn faded (another clone has the focus). */
  dim?: (name: string) => number;
  selected?: string;
}) {
  const frame = useCurrentFrame();
  return (
    <g>
      {spots.map((s) => {
        const p = place(s, bx, by, r, ring, length);
        const t = s.at === undefined ? 1 : ramp(frame, s.at, s.at + 18);
        const grow = easeOut(ramp(frame, (s.at ?? -99) + 8, (s.at ?? -99) + 24));
        const o = dim ? dim(s.name) : 1;
        const len = s.length ?? length;
        const cos = Math.cos(p.th);
        const sin = Math.sin(p.th);
        const off = len * 0.75;
        const lx = p.x + cos * off;
        const ly = p.y + sin * off;
        const align = cos > 0.35 ? "start" : cos < -0.35 ? "end" : "middle";
        const ldy = align === "middle" ? (sin < 0 ? -labelSize * 1.6 : labelSize * 0.6) : -labelSize * 0.5;
        return (
          <g key={s.name} opacity={o}>
            <Stem x0={p.x0} y0={p.y0} x1={p.x1} y1={p.y1} bend={0.1} draw={ease(t)} old={!!s.age} />
            {grow > 0 && <Clove x={p.x} y={p.y} angle={p.angle} length={len} state={s.state} age={s.age} skins={s.skins} grow={grow} selected={selected === s.name} />}
            <Label x={lx} y={ly + ldy} name={s.name} parts={s.parts} align={align} size={labelSize} opacity={grow} selected={selected === s.name} />
          </g>
        );
      })}
    </g>
  );
}
