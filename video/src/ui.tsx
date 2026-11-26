import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";
import { loadFont as loadSerif } from "@remotion/google-fonts/Newsreader";
import { createContext, useContext, type CSSProperties, type ReactNode } from "react";
import { AbsoluteFill, Easing, interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";

export const mono = loadMono("normal", { weights: ["400", "500", "600"] }).fontFamily;
export const serif = loadSerif("normal", { weights: ["400", "500"] }).fontFamily;
export const serifItalic = loadSerif("italic", { weights: ["400"] }).fontFamily;

/** Bottle, the clove palette (brand/README.md), dark. */
export const c = {
  paper: "#0d110f",
  deep: "#090c0a",
  ink: "#e9eee8",
  muted: "#8d9891",
  dim: "#5b6660",
  rule: "rgba(228,234,227,0.09)",
  bottle: "#7cc49b",
  bottleDeep: "#2f6b4c",
  peel: "#e3c95e",
  red: "#e8806f",
};

import words from "./words.json";

export const sec = (s: number) => Math.round(s * 30);

/** The current scene's length in frames. useVideoConfig() gives the whole
 * video's length inside a TransitionSeries, so Root provides each scene's. */
export const SceneLength = createContext(300);
export const useSceneLength = () => useContext(SceneLength);

/** Frames into a scene before its voice starts (Root uses the same). */
export const LEAD = 10;

/** The frame, within scene `scene`, where its voice says `word` (the `nth` time). */
export function cue(scene: string, word: string, nth = 0, offset = 0): number {
  const hits = ((words as Record<string, { w: string; t: number }[]>)[scene] ?? []).filter((x) => x.w === word);
  const hit = hits[nth] ?? hits[hits.length - 1];
  if (!hit) throw new Error(`no "${word}" in the ${scene} line`);
  return LEAD + Math.round(hit.t * 30) + offset;
}

/** 0 → 1 with a soft spring, starting at `delay` frames. */
export function useIn(delay = 0, damping = 200, mass = 1) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return spring({ frame: frame - delay, fps, config: { damping, mass } });
}

/** Linear 0 → 1 between two frames, clamped, eased. */
export function useRamp(from: number, to: number, ease = Easing.inOut(Easing.cubic)) {
  const frame = useCurrentFrame();
  return interpolate(frame, [from, to], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: ease });
}

/** The living backdrop under every scene: drifting light, dot grid, grain, vignette. */
export function Backdrop() {
  const frame = useCurrentFrame();
  const blob = (x: number, y: number, r: number, color: string, speed: number, phase: number) => {
    const t = frame * speed + phase;
    return (
      <div
        style={{
          position: "absolute",
          left: x + Math.sin(t) * 120,
          top: y + Math.cos(t * 0.8) * 90,
          width: r,
          height: r,
          borderRadius: "50%",
          background: color,
          filter: "blur(140px)",
          opacity: 0.55,
        }}
      />
    );
  };
  return (
    <AbsoluteFill style={{ background: c.paper, overflow: "hidden" }}>
      {blob(-200, -260, 900, "rgba(47,107,76,0.55)", 0.006, 0)}
      {blob(1150, 520, 820, "rgba(227,201,94,0.16)", 0.005, 2)}
      {blob(700, 150, 700, "rgba(124,196,155,0.18)", 0.004, 4)}
      <AbsoluteFill
        style={{
          backgroundImage: "radial-gradient(rgba(233,238,232,0.06) 1px, transparent 1px)",
          backgroundSize: "34px 34px",
          maskImage: "radial-gradient(ellipse at 50% 45%, black 20%, transparent 75%)",
        }}
      />
      <AbsoluteFill
        style={{
          opacity: 0.07,
          backgroundImage:
            "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='220' height='220'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>\")",
        }}
      />
      <AbsoluteFill style={{ background: "radial-gradient(ellipse at 50% 50%, transparent 55%, rgba(0,0,0,0.55) 100%)" }} />
    </AbsoluteFill>
  );
}

/** A scene's stage: transparent over the backdrop, with a slow camera push. */
export function Stage({ children, push = 0.035, style }: { children: ReactNode; push?: number; style?: CSSProperties }) {
  const frame = useCurrentFrame();
  const durationInFrames = useSceneLength();
  const s = 1 + push * (frame / durationInFrames);
  return (
    <AbsoluteFill
      style={{
        color: c.ink,
        fontFamily: serif,
        alignItems: "center",
        justifyContent: "center",
        transform: `scale(${s})`,
        ...style,
      }}
    >
      {children}
    </AbsoluteFill>
  );
}

/** Small mono label above a headline: THE PROBLEM, THE FIX. */
export function Kicker({ children, delay = 0, color = c.bottle }: { children: ReactNode; delay?: number; color?: string }) {
  const p = useIn(delay);
  return (
    <div
      style={{
        fontFamily: mono,
        fontSize: 20,
        letterSpacing: 6,
        textTransform: "uppercase",
        color,
        opacity: p,
        transform: `translateY(${(1 - p) * 10}px)`,
        display: "flex",
        alignItems: "center",
        gap: 14,
      }}
    >
      <span style={{ width: 28 * p, height: 1, background: color, display: "inline-block" }} />
      {children}
    </div>
  );
}

/** Headline whose words rise in one after another. */
export function Words({
  text,
  delay = 0,
  size = 76,
  stagger = 3,
  color = c.ink,
  accent,
  style,
}: {
  text: string;
  delay?: number;
  size?: number;
  stagger?: number;
  color?: string;
  /** Words (exact) to colour green. */
  accent?: string[];
  style?: CSSProperties;
}) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <div style={{ fontFamily: serif, fontSize: size, lineHeight: 1.08, letterSpacing: -size * 0.02, color, ...style }}>
      {text.split(" ").map((w, i) => {
        const p = spring({ frame: frame - delay - i * stagger, fps, config: { damping: 200 } });
        const hot = accent?.includes(w.replace(/[.,]/g, ""));
        return (
          <span
            key={i}
            style={{
              display: "inline-block",
              marginRight: size * 0.24,
              opacity: p,
              transform: `translateY(${(1 - p) * size * 0.45}px)`,
              filter: `blur(${(1 - p) * 6}px)`,
              color: hot ? c.bottle : undefined,
              fontStyle: hot ? "italic" : undefined,
            }}
          >
            {w}
          </span>
        );
      })}
    </div>
  );
}

export function Terminal({
  title,
  children,
  width = 1100,
  delay = 0,
  glow = c.bottle,
  style,
}: {
  title: string;
  children: ReactNode;
  width?: number;
  delay?: number;
  glow?: string;
  style?: CSSProperties;
}) {
  const p = useIn(delay, 22);
  return (
    <div
      style={{
        width,
        borderRadius: 20,
        background: "linear-gradient(180deg, rgba(20,27,23,0.92), rgba(9,12,10,0.94))",
        border: `1px solid ${c.rule}`,
        boxShadow: `0 50px 140px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.03) inset, 0 0 80px ${glow}14`,
        backdropFilter: "blur(10px)",
        overflow: "hidden",
        opacity: Math.min(1, p * 1.4),
        transform: `translateY(${(1 - p) * 40}px) scale(${0.97 + 0.03 * p})`,
        ...style,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "16px 20px", borderBottom: `1px solid ${c.rule}` }}>
        {["#ec6a5e", "#f4bf4f", "#61c554"].map((bg) => (
          <div key={bg} style={{ width: 13, height: 13, borderRadius: 7, background: bg, opacity: 0.35 }} />
        ))}
        <div style={{ flex: 1, textAlign: "center", fontFamily: mono, fontSize: 17, color: c.dim, marginRight: 60 }}>
          {title}
        </div>
      </div>
      <div style={{ padding: "26px 34px 30px", fontFamily: mono, fontSize: 26, lineHeight: 1.7 }}>{children}</div>
    </div>
  );
}

/** Text typed out from `start`, `cps` characters a second, with a caret while typing. */
export function Typed({ text, start = 0, cps = 34, prompt = true }: { text: string; start?: number; cps?: number; prompt?: boolean }) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (frame < start) return null;
  const n = Math.floor(((frame - start) / fps) * cps);
  const done = n >= text.length;
  const caret = !done || Math.floor(frame / 15) % 2 === 0;
  const shown = text.slice(0, n);
  const [cmd, ...rest] = shown.split(" ");
  return (
    <div style={{ whiteSpace: "pre" }}>
      {prompt && <span style={{ color: c.bottle }}>❯ </span>}
      <span style={{ color: c.ink, fontWeight: 600 }}>{cmd}</span>
      {rest.length > 0 && <span style={{ color: c.muted }}> {rest.join(" ")}</span>}
      <span style={{ opacity: caret ? 0.9 : 0, color: c.bottle }}>▍</span>
    </div>
  );
}

/** A line that slides in at `at`. */
export function Out({ at, children, color = c.muted, style }: { at: number; children: ReactNode; color?: string; style?: CSSProperties }) {
  const frame = useCurrentFrame();
  if (frame < at) return null;
  const o = interpolate(frame, [at, at + 7], [0, 1], { extrapolateRight: "clamp" });
  return (
    <div style={{ whiteSpace: "pre", color, opacity: o, transform: `translateX(${(1 - o) * -10}px)`, ...style }}>
      {children}
    </div>
  );
}

/** The clove mark: a c (the golden image) holding a clove (your copy). */
export function Mark({ size = 220, delay = 0, alive = false }: { size?: number; delay?: number; alive?: boolean }) {
  const frame = useCurrentFrame();
  const arc = interpolate(frame, [delay, delay + 26], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });
  const leaf = useIn(delay + 16, 11);
  const len = 2 * Math.PI * 31 * 0.68;
  // Alive: the leaf buds three copies that drift off (clones), then sways and breathes.
  const t = Math.max(0, frame - delay - 30);
  const sway = alive ? Math.sin(t / 16) * 5 * Math.min(1, t / 20) : 0;
  const glow = alive ? 3 + 1.6 * (0.5 + 0.5 * Math.sin(t / 11)) : 3;
  // Directions in degrees clockwise from up: out through the C's opening.
  const buds = alive ? [76, 90, 104] : [];
  const leafAt = (rot: number, sc: number) => `translate(53 51) rotate(${rot}) scale(${sc}) translate(-50 -49)`;
  return (
    <svg width={size} height={size} viewBox="0 0 100 100" style={{ overflow: "visible" }}>
      <defs>
        <filter id="leafglow" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation={glow} result="b" />
          <feMerge>
            <feMergeNode in="b" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>
      {buds.map((ang, i) => {
        const b = interpolate(frame, [delay + 30 + i * 4, delay + 62 + i * 4], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.quad) });
        if (b <= 0 || b >= 1) return null;
        const r = ang * (Math.PI / 180);
        return (
          <g key={i} transform={`translate(${Math.sin(r) * 40 * b} ${-Math.cos(r) * 40 * b})`} opacity={(1 - b) * 0.8}>
            <g transform={leafAt(16 + (ang - 90) * 0.6 * b, 0.33 * (1 - b * 0.35))}>
              <path d="M50 8 C62 28 74 52 70 76 C68 90 32 90 30 76 C26 52 38 28 50 8Z" fill={c.bottle} />
            </g>
          </g>
        );
      })}
      <path
        d="M73.04 29.26 A31 31 0 1 0 73.04 70.74"
        fill="none"
        stroke={c.ink}
        strokeWidth={19}
        strokeLinecap="round"
        strokeDasharray={len}
        strokeDashoffset={len * (1 - arc)}
        // A zero-length dash still draws its round cap: hide it until the arc starts.
        opacity={Math.min(1, arc * 10)}
      />
      <g filter="url(#leafglow)" transform={leafAt(16 - (1 - leaf) * 40 + sway, 0.33 * leaf)}>
        <path d="M50 8 C62 28 74 52 70 76 C68 90 32 90 30 76 C26 52 38 28 50 8Z" fill={c.bottle} />
      </g>
    </svg>
  );
}

export function Bar({ value, color = c.bottle, width = 520, height = 12 }: { value: number; color?: string; width?: number; height?: number }) {
  const v = Math.max(0, Math.min(1, value));
  return (
    <div style={{ width, height, borderRadius: height, background: "rgba(233,238,232,0.07)", overflow: "hidden" }}>
      <div
        style={{
          width: `${v * 100}%`,
          height: "100%",
          borderRadius: height,
          background: `linear-gradient(90deg, ${color}aa, ${color})`,
          boxShadow: `0 0 18px ${color}66`,
        }}
      />
    </div>
  );
}

/** A frosted card. */
export function Glass({ children, style, tint = "rgba(233,238,232,0.035)" }: { children: ReactNode; style?: CSSProperties; tint?: string }) {
  return (
    <div
      style={{
        borderRadius: 20,
        background: tint,
        border: `1px solid ${c.rule}`,
        boxShadow: "0 30px 90px rgba(0,0,0,0.45)",
        backdropFilter: "blur(12px)",
        ...style,
      }}
    >
      {children}
    </div>
  );
}
