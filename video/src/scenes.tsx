import { useEffect, useState } from "react";
import {
  continueRender,
  delayRender,
  Easing,
  getStaticFiles,
  Img,
  interpolate,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { Bar, c, cue, Glass, Kicker, Mark, mono, Out, sec, serif, Stage, Terminal, Typed, useIn, useRamp, useSceneLength, Words } from "./ui";

const has = (name: string) => getStaticFiles().some((f) => f.name === name);

/* ---------------------------------------------------------------- opening */

const SESSIONS: [string, string, number, number][] = [
  ["claude", "fix/race", 200, 300],
  ["codex", "agent/parser", 1390, 280],
  ["claude", "feat/login", 330, 660],
  ["cursor", "agent/docs", 1280, 640],
  ["claude", "pr/4053", 795, 450],
];

/** What an agent is up to, cycling on each card's own schedule. */
const ACTIVITY = ["thinking…", "reading src/api.ts", "editing 3 files", "running tests", "thinking…", "grep -r \"login\"", "writing a plan"];

function Session({ tool, branch, x, y, at }: { tool: string; branch: string; x: number; y: number; at: number }) {
  const frame = useCurrentFrame();
  const p = useIn(at, 12);
  // Each card floats on its own path: a slow loop in both axes and a slight tilt.
  const t = (frame + at * 11) / 30;
  const k = 0.8 + ((at * 7) % 5) * 0.12;
  const dx = Math.sin(t * 0.9 * k) * 9;
  const dy = Math.sin(t * 1.3 * k + 1) * 11;
  const tilt = Math.sin(t * 0.7 * k + 2) * 1.6;
  const spin = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"[Math.floor(frame / 3 + at) % 10];
  // Same green edge and glow as the branch lines, breathing out of step per card.
  const glow = 0.5 + 0.5 * Math.sin((frame + at * 7) / 14);
  // The status line changes every ~1.3s, offset per card; each new one fades in.
  const period = 38 + (at % 4) * 5;
  const step = Math.floor((frame + at * 13) / period);
  const since = (frame + at * 13) % period;
  const activity = ACTIVITY[(step + at) % ACTIVITY.length];
  const fresh = Math.min(1, since / 6);
  return (
    <Glass
      tint="rgba(124,196,155,0.06)"
      style={{
        position: "absolute",
        left: x + dx,
        top: y + dy,
        width: 330,
        padding: "18px 22px",
        fontFamily: mono,
        opacity: p,
        transform: `scale(${0.7 + 0.3 * p}) rotate(${tilt}deg)`,
        border: `1.5px solid ${c.bottle}88`,
        boxShadow: `0 0 ${18 + 16 * glow}px ${c.bottle}${glow > 0.5 ? "40" : "2e"}, 0 ${24 + dy}px 60px rgba(0,0,0,0.45)`,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 18 }}>
        <span style={{ color: c.bottle }}>{tool}</span>
        <span style={{ color: c.peel, fontSize: 26, lineHeight: "20px" }}>{spin}</span>
      </div>
      <div style={{ fontSize: 20, marginTop: 8 }}>{branch}</div>
      <div style={{ fontSize: 14, color: c.muted, marginTop: 6, whiteSpace: "nowrap", opacity: 0.45 + 0.55 * fresh, transform: `translateY(${(1 - fresh) * 4}px)` }}>{activity}</div>
    </Glass>
  );
}

/** 1. So, you're running agents. */
export function Intro() {
  const frame = useCurrentFrame();
  const t0 = cue("intro", "tokenmaxxing", 0, -4);
  const tokens = frame < t0 ? 0 : Math.floor(Math.pow(frame - t0, 2.6) * 37 + (frame - t0) * 9113);
  const tp = useIn(t0, 18);
  return (
    <Stage push={0.03}>
      <div style={{ position: "absolute", top: 120, left: 0, right: 0, display: "flex", justifyContent: "center" }}>
        <Words text="So, you've got a few clankers running." size={70} delay={4} accent={["clankers"]} />
      </div>
      {SESSIONS.map(([tool, branch, x, y], i) => (
        <Session key={branch} tool={tool} branch={branch} x={x} y={y} at={cue("intro", "clankers", 0, -6) + i * 5} />
      ))}
      <div
        style={{
          position: "absolute",
          bottom: 110,
          left: 0,
          right: 0,
          textAlign: "center",
          opacity: tp,
          transform: `translateY(${(1 - tp) * 20}px)`,
        }}
      >
        <span style={{ fontFamily: mono, fontSize: 64, color: c.peel, letterSpacing: -1 }}>{tokens.toLocaleString("en-US")}</span>
        <span style={{ fontFamily: mono, fontSize: 28, color: c.muted, marginLeft: 18 }}>tokens</span>
      </div>
    </Stage>
  );
}

/** 2. ...in parallel, each on its own branch. */
export function Parallel() {
  const frame = useCurrentFrame();
  const fork = cue("parallel", "parallel", 0, -6);
  const named = cue("parallel", "branch", 0, -4);
  const main = useRamp(4, fork);
  // The earliest fork takes the outermost lane, so no branch crosses another.
  const branches: [string, string, number, number][] = [
    ["feat/login", "claude", 420, 260],
    ["fix/race", "codex", 640, 840],
    ["agent/parser", "claude", 860, 410],
    ["agent/docs", "cursor", 1080, 690],
  ];
  return (
    <Stage push={0.025}>
      <div style={{ position: "absolute", top: 110, left: 0, right: 0, display: "flex", justifyContent: "center" }}>
        <Words text="In parallel, each on its own branch." size={64} delay={cue("parallel", "need", 0, -6)} accent={["parallel,"]} />
      </div>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0, overflow: "visible" }}>
        <defs>
          <linearGradient id="branch" x1="0" x2="1">
            <stop offset="0" stopColor={c.ink} stopOpacity="0.55" />
            <stop offset="0.35" stopColor={c.bottle} />
            <stop offset="1" stopColor={c.bottle} />
          </linearGradient>
          <filter id="branchglow" x="-20%" y="-50%" width="140%" height="200%">
            <feGaussianBlur stdDeviation="6" result="b" />
            <feMerge>
              <feMergeNode in="b" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>
        <line x1={200} y1={540} x2={200 + 1520 * main} y2={540} stroke={c.ink} strokeOpacity={0.45} strokeWidth={7} strokeLinecap="round" />
        {branches.map(([, , x, y], i) => {
          const d = interpolate(frame, [fork + i * 5, fork + i * 5 + 18], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) });
          const path = `M ${x} 540 C ${x + 150} 540, ${x + 110} ${y}, ${x + 270} ${y} L ${x + 620} ${y}`;
          const dots = [x + 360, x + 470, x + 580];
          return (
            <g key={i}>
              <path
                d={path}
                fill="none"
                stroke="url(#branch)"
                strokeWidth={6}
                strokeLinecap="round"
                filter="url(#branchglow)"
                pathLength={1}
                strokeDasharray={1}
                strokeDashoffset={1 - d}
                opacity={d > 0 ? 1 : 0}
              />
              {dots.map((cx, j) => {
                const p = interpolate(frame, [fork + i * 5 + 14 + j * 4, fork + i * 5 + 22 + j * 4], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
                return <circle key={j} cx={cx} cy={y} r={7 * p} fill={c.paper} stroke={c.bottle} strokeWidth={3} />;
              })}
              <circle cx={x} cy={540} r={11} fill={c.paper} stroke={c.ink} strokeWidth={4} opacity={d > 0 ? 1 : 0} />
            </g>
          );
        })}
        <text x={180} y={594} fill={c.muted} fontFamily={mono} fontSize={22} opacity={main}>
          main
        </text>
      </svg>
      {branches.map(([name, tool, x, y], i) => {
        const at = named + i * 4;
        const p = interpolate(frame, [at, at + 10], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
        return (
          <div
            key={name}
            style={{
              position: "absolute",
              left: x + 290,
              top: y - (y < 540 ? 58 : -18),
              fontFamily: mono,
              fontSize: 22,
              opacity: p,
              display: "flex",
              gap: 12,
              alignItems: "center",
            }}
          >
            <span style={{ color: c.ink }}>{name}</span>
            <span style={{ color: c.bottle, fontSize: 18, padding: "3px 10px", borderRadius: 999, border: `1px solid ${c.bottle}55` }}>{tool}</span>
          </div>
        );
      })}
    </Stage>
  );
}

/** 2b. One checkout shared by every agent: they clash. */
export function Clash() {
  const frame = useCurrentFrame();
  const k = (w: string, n = 0, o = 0) => cue("clash", w, n, o);
  const card = useIn(4, 18);
  const hit = k("clash", 0, -2);
  const red = interpolate(frame, [hit, hit + 8], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const shake = frame >= hit && frame < hit + 14 ? Math.sin((frame - hit) * 2.4) * 6 * (1 - (frame - hit) / 14) : 0;
  const sw = k("switches", 0, -2);
  const cycle = ["feat/login", "agent/parser", "agent/docs", "fix/race"];
  const head = frame < sw ? cycle[0] : cycle[Math.min(cycle.length - 1, Math.floor((frame - sw) / 5))];
  const agents: [string, string, number, number][] = [
    ["claude", "feat/login", 170, 300],
    ["codex", "fix/race", 1450, 300],
    ["claude", "agent/parser", 170, 680],
    ["cursor", "agent/docs", 1450, 680],
  ];
  const wire = `rgb(${Math.round(124 + (232 - 124) * red)},${Math.round(196 + (128 - 196) * red)},${Math.round(155 + (111 - 155) * red)})`;
  return (
    <Stage push={0.025}>
      <div style={{ position: "absolute", top: 110, left: 0, right: 0, display: "flex", justifyContent: "center" }}>
        <Words text="One checkout. Everyone in it." size={64} delay={k("but", 0, -4)} accent={["Everyone"]} />
      </div>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        {agents.map(([, , x, y], i) => {
          const p = interpolate(frame, [10 + i * 4, 26 + i * 4], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
          const x0 = x < 960 ? x + 340 : x;
          const x1 = x < 960 ? 600 : 1320;
          return <line key={i} x1={x0} y1={y + 40} x2={x0 + (x1 - x0) * p} y2={y + 40 + (520 - y - 40) * p} stroke={wire} strokeOpacity={0.6} strokeWidth={2.5} />;
        })}
      </svg>
      {agents.map(([tool, branch, x, y], i) => {
        const p = useIn(6 + i * 4, 16);
        return (
          <Glass key={i} style={{ position: "absolute", left: x, top: y, width: 340, padding: "16px 22px", fontFamily: mono, opacity: p, border: `1px solid ${red > 0.5 ? `${c.red}55` : c.rule}` }}>
            <div style={{ fontSize: 17, color: c.bottle }}>{tool}</div>
            <div style={{ fontSize: 19, marginTop: 4, whiteSpace: "nowrap" }}>wants {branch}</div>
          </Glass>
        );
      })}
      <Glass
        style={{
          position: "absolute",
          left: 600,
          top: 360,
          width: 720,
          padding: "30px 36px",
          fontFamily: mono,
          opacity: card,
          transform: `translateX(${shake}px) scale(${0.95 + 0.05 * card})`,
          border: `1px solid ${red > 0.5 ? `${c.red}77` : c.rule}`,
          boxShadow: red > 0.5 ? `0 0 70px ${c.red}22` : "none",
        }}
      >
        <div style={{ fontSize: 18, color: c.dim }}>~/src/mono · the only checkout</div>
        <div style={{ fontSize: 40, marginTop: 14 }}>
          <span style={{ color: c.muted }}>HEAD → </span>
          <span style={{ color: frame >= sw ? c.red : c.ink }}>{head}</span>
        </div>
        <div style={{ fontSize: 18, marginTop: 18, lineHeight: 1.75 }}>
          <Out at={sw} color={c.muted}>codex ❯ git checkout fix/race</Out>
          <Out at={k("login", 0, -2)} color={c.red}>✗ claude's login fix, now on codex's fix/race</Out>
        </div>
      </Glass>
    </Stage>
  );
}

/* ---------------------------------------------------------------- problem */

/** The viewer's own retort, as a speech bubble. */
function Quote({ at }: { at: number }) {
  const p = useIn(at, 14);
  return (
    <div
      style={{
        position: "absolute",
        top: 280,
        left: 370,
        opacity: p,
        transform: `translateY(${(1 - p) * 20}px) rotate(${(1 - p) * -3}deg)`,
        transformOrigin: "bottom left",
      }}
    >
      <Glass tint="rgba(124,196,155,0.08)" style={{ padding: "22px 34px", borderRadius: "26px 26px 26px 6px", border: `1px solid ${c.bottle}55` }}>
        <span style={{ fontFamily: serif, fontSize: 52, fontStyle: "italic" }}>
          “I'm not a <span style={{ color: c.bottle }}>caveman</span>.”
        </span>
      </Glass>
    </div>
  );
}

/** 3. Fret not: git has worktrees. */
export function Worktree() {
  return (
    <Stage>
      <Quote at={cue("worktree", "i'm", 0, -6)} />
      <Terminal title="~/src/mono" width={1180} delay={cue("worktree", "git", 0, -10)} style={{ marginTop: 120 }}>
        <Typed text="git worktree add ../feat-login -b feat/login" start={cue("worktree", "git", 0, -4)} />
        <Out at={cue("worktree", "fresh")}>Preparing worktree (new branch 'feat/login')</Out>
        <Out at={cue("worktree", "fresh", 0, 5)}>HEAD is now at c9483ee feat(api): rate limits per team</Out>
        <Out at={cue("worktree", "second")} color={c.bottle}>
          ✓ ready in 0.9s
        </Out>
      </Terminal>
    </Stage>
  );
}

function Missing({ label, hint, at }: { label: string; hint: string; at: number }) {
  const frame = useCurrentFrame();
  const p = useIn(at, 16);
  const strike = interpolate(frame, [at + 8, at + 18], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <Glass tint="rgba(232,128,111,0.05)" style={{ padding: "22px 30px", width: 470, opacity: p, transform: `translateX(${(1 - p) * 60}px)` }}>
      <div style={{ display: "flex", alignItems: "center", gap: 18, fontFamily: mono, fontSize: 30 }}>
        <span style={{ color: c.red }}>✗</span>
        <span style={{ position: "relative", color: c.ink }}>
          {label}
          <span style={{ position: "absolute", left: 0, top: "54%", height: 3, width: `${strike * 100}%`, background: c.red, borderRadius: 2 }} />
        </span>
      </div>
      <div style={{ fontFamily: mono, fontSize: 15, color: c.muted, marginTop: 8, marginLeft: 44, whiteSpace: "nowrap" }}>{hint}</div>
    </Glass>
  );
}

/** 2. ...but empty. */
/** A tumbleweed bouncing and rolling across the bottom of the frame. */
function Tumbleweed({ at }: { at: number }) {
  const frame = useCurrentFrame();
  const t = frame - at;
  if (t < 0) return null;
  const DUR = 75;
  const p = Math.min(1, t / DUR);
  const x = interpolate(p, [0, 1], [-160, 2080]);
  // Bounces that fade as it settles into a roll.
  const hop = Math.abs(Math.sin(t / 7)) * 46 * Math.max(0.25, 1 - p * 0.9);
  const ground = 965;
  const r = 52;
  const spin = (x / r) * (180 / Math.PI);
  // A dry tangle: fixed pseudo-random twigs, many crossing the middle, a few
  // poking out past the rim.
  const rnd = (n: number) => {
    const v = Math.sin(n * 12.9898) * 43758.5453;
    return v - Math.floor(v);
  };
  const twigs = Array.from({ length: 34 }, (_, i) => {
    const a = rnd(i + 1) * Math.PI * 2;
    const len = 0.7 + rnd(i + 50) * 1.6;
    const b = a + Math.PI * len * (rnd(i + 90) > 0.5 ? 1 : -1);
    const ra = r * (0.35 + rnd(i + 7) * 0.75);
    const rb = r * (0.35 + rnd(i + 13) * 0.75);
    const pull = r * (rnd(i + 21) - 0.5) * 0.9;
    const cx = Math.cos((a + b) / 2) * pull;
    const cy = Math.sin((a + b) / 2) * pull;
    return `M ${(Math.cos(a) * ra).toFixed(1)} ${(Math.sin(a) * ra).toFixed(1)} Q ${cx.toFixed(1)} ${cy.toFixed(1)} ${(Math.cos(b) * rb).toFixed(1)} ${(Math.sin(b) * rb).toFixed(1)}`;
  });
  const spikes = Array.from({ length: 9 }, (_, i) => {
    const a = rnd(i + 200) * Math.PI * 2;
    const r1 = r * 0.75;
    const r2 = r * (1.05 + rnd(i + 300) * 0.3);
    const bend = (rnd(i + 400) - 0.5) * 0.5;
    return `M ${(Math.cos(a) * r1).toFixed(1)} ${(Math.sin(a) * r1).toFixed(1)} L ${(Math.cos(a + bend) * r2).toFixed(1)} ${(Math.sin(a + bend) * r2).toFixed(1)}`;
  });
  const dust = [0, 1, 2].map((k) => {
    const d = (t - k * 7) % 21;
    return { o: d < 0 ? 0 : Math.max(0, 0.35 - d / 60), s: 6 + d * 0.9, dx: -d * 2.2 - k * 14 };
  });
  return (
    <svg width={1920} height={1080} style={{ position: "absolute", inset: 0, overflow: "visible", pointerEvents: "none" }}>
      <ellipse cx={x} cy={ground + r + 4} rx={r * (0.9 - hop / 160)} ry={7} fill="rgba(0,0,0,0.45)" opacity={0.9 - hop / 90} />
      {dust.map((g, k) => (
        <circle key={k} cx={x + g.dx} cy={ground + r - 2} r={g.s} fill="#b9a675" opacity={hop < 6 ? g.o : g.o * 0.4} />
      ))}
      <g transform={`translate(${x} ${ground - hop}) rotate(${spin})`}>
        <circle r={r * 0.8} fill="rgba(169,146,96,0.1)" />
        {twigs.map((d, i) => (
          <path key={d} d={d} fill="none" stroke={["#a8915c", "#c9b27a", "#8a7547"][i % 3]} strokeWidth={i % 5 ? 1.8 : 2.8} strokeLinecap="round" opacity={0.85} />
        ))}
        {spikes.map((d) => (
          <path key={d} d={d} fill="none" stroke="#b39d68" strokeWidth={1.6} strokeLinecap="round" opacity={0.8} />
        ))}
      </g>
    </svg>
  );
}

export function Empty() {
  return (
    <Stage>
      <Tumbleweed at={cue("empty", "tumbleweeds", 0, -8)} />
      <div style={{ display: "flex", gap: 56, alignItems: "center" }}>
        <div>
          <Kicker delay={0} color={c.red}>
            But
          </Kicker>
          <div style={{ height: 28 }} />
          <Terminal title="~/src/feat-login" width={900} glow={c.red}>
            <Typed text="make dev" start={8} />
            <Out at={cue("empty", "empty")} color={c.red}>
              sh: next: command not found
            </Out>
            <Out at={cue("empty", "empty", 0, 5)} color={c.red}>
              ModuleNotFoundError: No module named 'fastapi'
            </Out>
            <Out at={cue("empty", "empty", 0, 10)} color={c.red}>
              protoc: command not found
            </Out>
            <Out at={cue("empty", "tumbleweeds", 0, -4)} color={c.dim}>
              · a tumbleweed rolls by
            </Out>
          </Terminal>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 20, marginTop: 50 }}>
          <Missing label="dependencies" hint="node_modules · .venv · target/" at={cue("empty", "dependencies", 0, -4)} />
          <Missing label="dev shell" hint="nix · mise · direnv" at={cue("empty", "dev", 0, -4)} />
          <Missing label="local files" hint=".env · certs · local config" at={cue("empty", "local", 0, -4)} />
        </div>
      </div>
    </Stage>
  );
}


/** 3. Getting ready: over a minute, gigabytes, every time. */
/** A glass tile with a small label, as on clove.fast's cost section. */
function GaugeTile({ label, caption, captionColor, children }: { label: string; caption: string; captionColor: string; children: React.ReactNode }) {
  return (
    <Glass style={{ width: 250, padding: "18px 18px 22px", display: "flex", flexDirection: "column", alignItems: "center" }}>
      <div style={{ alignSelf: "flex-start", fontFamily: mono, fontSize: 14, letterSpacing: 4, color: c.muted, marginBottom: 14 }}>{label}</div>
      {children}
      <div style={{ fontFamily: mono, fontSize: 17, color: captionColor, marginTop: 14, whiteSpace: "nowrap" }}>{caption}</div>
    </Glass>
  );
}

/** A circle arc from 12 o'clock, `from` to `to` in turns. */
function Arc({ r, from, to, color, width }: { r: number; from: number; to: number; color: string; width: number }) {
  const len = Math.max(0, to - from);
  if (len <= 0) return null;
  return (
    <circle r={r} fill="none" stroke={color} strokeWidth={width} pathLength={1} strokeDasharray={`${len} ${1 - len}`} strokeDashoffset={-from} transform="rotate(-90)" />
  );
}

/** 4. Over a minute and gigabytes, per worktree: time, disk, fan. */
export function Cost() {
  const frame = useCurrentFrame();
  const durationInFrames = useSceneLength();
  const fanAt = cue("cost", "fan", 0, -6);
  const k = useRamp(10, durationInFrames - 34, Easing.inOut(Easing.quad));
  const t = k * 72;
  const rows: [string, number, number][] = [
    ["dev shell · nix + direnv", 0, 41],
    ["dependencies · pnpm, uv, cargo", 41, 71],
    ["local files · .env", 71, 72],
  ];
  // Fan: idles, then spins up on "fan". The angle is the running sum of the
  // speed; capped well under a blade's spacing per frame, so it never strobes.
  const speed = (f: number) => interpolate(f, [fanAt, fanAt + 24], [3, 21], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  let angle = 0;
  for (let f = 0; f < frame; f++) angle += speed(f);
  const rpm = Math.round(interpolate(frame, [fanAt, fanAt + 24], [1200, 6800], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) + (frame > fanAt ? Math.sin(frame * 1.7) * 60 : 0));
  const hot = frame >= fanAt;
  const shake = hot ? Math.sin(frame * 2.3) * 1.2 : 0;
  const R = 62;
  const free = 4.5 - k * 3.1;
  return (
    <Stage>
      <div style={{ position: "absolute", top: 130, left: 230 }}>
        <Kicker delay={2} color={c.peel}>
          Every new worktree
        </Kicker>
      </div>
      <div style={{ display: "flex", gap: 90, alignItems: "center", marginTop: 40 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 40, fontFamily: mono, fontSize: 22 }}>
          {rows.map(([label, from, to]) => {
            const v = (t - from) / (to - from);
            const active = v > 0 && v < 1;
            return (
              <div key={label} style={{ opacity: t >= from - 0.5 ? 1 : 0.35 }}>
                <div style={{ display: "flex", justifyContent: "space-between", color: active ? c.ink : c.muted, marginBottom: 14 }}>
                  <span>{label}</span>
                  <span style={{ color: c.dim }}>{Math.round(Math.max(0, Math.min(t, to) - from))}s</span>
                </div>
                <Bar value={v} width={600} color={v >= 1 ? c.dim : c.peel} />
              </div>
            );
          })}
        </div>
        <div style={{ display: "flex", gap: 18 }}>
          <GaugeTile label="TIME" caption="per worktree" captionColor={c.peel}>
            <svg width={170} height={170} viewBox="-85 -85 170 170">
              <circle r={R} fill="none" stroke="rgba(233,238,232,0.07)" strokeWidth={16} />
              {[0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330].map((d) => (
                <line key={d} y1={-R + 13} y2={-R + 18} stroke={c.dim} strokeWidth={2} transform={`rotate(${d})`} />
              ))}
              <Arc r={R} from={0} to={Math.min(t, 60) / 60} color={c.peel} width={16} />
              <Arc r={R - 17} from={0} to={Math.max(0, t - 60) / 60} color={c.red} width={7} />
              <line y2={-R + 22} stroke={c.ink} strokeWidth={3.5} strokeLinecap="round" transform={`rotate(${(t / 60) * 360})`} />
              <circle r={5} fill={c.ink} />
              <text y={36} textAnchor="middle" fill={c.ink} fontFamily={mono} fontSize={20}>
                {Math.round(t)}s
              </text>
            </svg>
          </GaugeTile>
          <GaugeTile label="DISK" caption={`+${(k * 3.1).toFixed(1)} GB`} captionColor={c.peel}>
            <svg width={170} height={170} viewBox="-85 -85 170 170">
              <circle r={R} fill="none" stroke="rgba(233,238,232,0.07)" strokeWidth={16} />
              <Arc r={R} from={0} to={0.55} color="rgba(233,238,232,0.2)" width={16} />
              <Arc r={R} from={0.55} to={0.55 + k * 0.31} color={k > 0.8 ? c.red : c.peel} width={16} />
              <text y={0} textAnchor="middle" fill={free < 2 ? c.red : c.ink} fontFamily={mono} fontSize={21}>
                {free.toFixed(1)} GB
              </text>
              <text y={22} textAnchor="middle" fill={c.muted} fontFamily={mono} fontSize={13}>
                free
              </text>
            </svg>
          </GaugeTile>
          <GaugeTile label="FAN" caption={`${rpm.toLocaleString("en-US")} rpm`} captionColor={hot ? c.red : c.muted}>
            <svg width={170} height={170} viewBox="-70 -70 140 140" style={{ transform: `translateX(${shake}px)`, color: hot ? c.red : c.muted }}>
              <circle r={64} fill="none" stroke="currentColor" strokeOpacity={0.25} strokeWidth={3} />
              <circle r={55} fill="none" stroke="currentColor" strokeOpacity={0.12} strokeWidth={1.5} strokeDasharray="2 5" />
              <g transform={`rotate(${angle})`}>
                {[0, 51, 103, 154, 206, 257, 309].map((a) => (
                  <path key={a} transform={`rotate(${a})`} d="M0 -8 C 18 -14, 44 -34, 50 -14 C 54 -1, 28 3, 0 8 Z" fill="currentColor" fillOpacity={0.75} />
                ))}
              </g>
              <circle r={10} fill={c.paper} stroke="currentColor" strokeWidth={3} />
            </svg>
          </GaugeTile>
        </div>
      </div>
    </Stage>
  );
}

/** One line of an agent's transcript, tagged with what it really is. */
function Said({ at, kind, text, tag }: { at: number; kind: "say" | "run" | "err"; text: string; tag?: [string, string] }) {
  const frame = useCurrentFrame();
  if (frame < at) return null;
  const p = interpolate(frame, [at, at + 8], [0, 1], { extrapolateRight: "clamp" });
  const tp = interpolate(frame, [at + 8, at + 16], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const mark = kind === "say" ? <span style={{ color: c.ink }}>●</span> : kind === "run" ? <span style={{ color: c.bottle }}>❯</span> : <span style={{ color: c.red }}>✗</span>;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 16, opacity: p, transform: `translateY(${(1 - p) * 10}px)`, marginBottom: 14 }}>
      <span style={{ width: 22 }}>{mark}</span>
      <span style={{ flex: 1, color: kind === "say" ? c.ink : kind === "err" ? c.red : c.muted, whiteSpace: "pre" }}>{text}</span>
      {tag && (
        <span
          style={{
            fontSize: 16,
            letterSpacing: 2,
            color: tag[1],
            padding: "4px 12px",
            borderRadius: 999,
            border: `1px solid ${tag[1]}66`,
            background: `${tag[1]}14`,
            opacity: tp,
            transform: `scale(${0.8 + 0.2 * tp})`,
          }}
        >
          {tag[0]}
        </span>
      )}
    </div>
  );
}

/** 5. The agent improvises: reinstalls, gives up, or hacks across checkouts. */
export function Hacks() {
  const k = (w: string, n = 0, o = -4) => cue("hacks", w, n, o);
  return (
    <Stage push={0.02}>
      <div style={{ position: "absolute", top: 100, left: 220 }}>
        <Kicker delay={2} color={c.peel}>
          So your agent improvises
        </Kicker>
      </div>
      <Terminal title="claude · feat/login" width={1340} glow={c.peel} style={{ marginTop: 50 }}>
        <div style={{ fontSize: 24 }}>
          <Said at={k("improvises", 0, -10)} kind="say" text="This worktree has no dependencies. Setting it up first." />
          <Said at={k("reinstalls")} kind="run" text="pnpm install                                  31s" tag={["REINSTALL", c.peel]} />
          <Said at={k("everything")} kind="run" text="direnv allow && nix develop                   41s" tag={["REINSTALL", c.peel]} />
          <Said at={k("lint")} kind="err" text="can't run lint here: eslint isn't installed" tag={["GAVE UP", c.red]} />
          <Said at={k("regenerate")} kind="err" text="can't regenerate the protos here: buf not on PATH" tag={["GAVE UP", c.red]} />
          <Said at={k("reaches")} kind="run" text="cd ../mono && make proto && cp -r gen ../feat-login/" tag={["HACK", c.red]} />
          <Said at={k("links")} kind="run" text="ln -s ../mono/node_modules node_modules" tag={["HACK", c.red]} />
        </div>
      </Terminal>
    </Stage>
  );
}

function RediscoverCard({ name, i }: { name: string; i: number }) {
  const frame = useCurrentFrame();
  const p = useIn(i * 4, 16);
  const steps = ["pnpm install", "nix develop", "uv sync", "cargo build", "make proto", "ln -s ../mono/…"];
  const step = steps[Math.floor((frame + i * 11) / 22) % steps.length];
  const spin = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"[Math.floor(frame / 3 + i) % 10];
  return (
    <Glass style={{ width: 300, padding: "24px 26px", fontFamily: mono, opacity: p, transform: `translateY(${(1 - p) * 30}px)` }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, fontSize: 22 }}>
        <span style={{ width: 9, height: 9, borderRadius: 5, background: c.peel, boxShadow: `0 0 10px ${c.peel}` }} />
        {name}
      </div>
      <div style={{ fontSize: 18, color: c.peel, marginTop: 16 }}>
        {spin} {step}
      </div>
    </Glass>
  );
}

/** 6. ...and the next agent does it all again. */
export function Again() {
  const names = ["agent/parser", "agent/cache", "fix/race", "agent/docs", "agent/tests"];
  return (
    <Stage>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 64 }}>
        <div style={{ display: "flex", gap: 24 }}>
          {names.map((n, i) => (
            <RediscoverCard key={n} name={n} i={i} />
          ))}
        </div>
        <Words text="From scratch, every single time." size={72} delay={cue("again", "scratch", 0, -8)} accent={["every", "single", "time."]} />
      </div>
    </Stage>
  );
}

/* --------------------------------------------------------------- solution */

/** 8. There's a better way: the mark. */
export function Logo() {
  const frame = useCurrentFrame();
  const at = cue("logo", "meet", 0, -6);
  const word = useIn(at + 14, 200);
  const glow = useRamp(at + 8, at + 38);
  const lead = useIn(4, 200) * interpolate(frame, [at - 8, at], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <Stage push={0.06}>
      <div
        style={{
          position: "absolute",
          width: 900,
          height: 900,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(124,196,155,0.22), transparent 62%)",
          opacity: glow,
        }}
      />
      <div style={{ position: "absolute", opacity: lead, transform: `translateY(${(1 - lead) * 16}px)` }}>
        <Words text="There's a better way." size={84} delay={4} accent={["better"]} />
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 40, opacity: frame >= at ? 1 : 0 }}>
        <Mark size={260} delay={at} />
        <div
          style={{
            fontFamily: serif,
            fontSize: 210,
            letterSpacing: -7,
            opacity: word,
            transform: `translateX(${(1 - word) * -40}px)`,
            filter: `blur(${(1 - word) * 8}px)`,
          }}
        >
          clove
        </div>
      </div>
    </Stage>
  );
}

function CloneCard({ label, at, x, y }: { label: string; at: number; x: number; y: number }) {
  const p = useIn(at, 16);
  return (
    <Glass
      tint="rgba(124,196,155,0.06)"
      style={{
        position: "absolute",
        left: x,
        top: y,
        width: 400,
        padding: "20px 26px",
        fontFamily: mono,
        fontSize: 23,
        display: "flex",
        justifyContent: "space-between",
        opacity: p,
        transform: `translateX(${(1 - p) * -80}px) scale(${0.9 + 0.1 * p})`,
      }}
    >
      <span>{label}</span>
      <span style={{ color: c.bottle }}>+6 MB</span>
    </Glass>
  );
}

/** A dot travelling along a cubic wire from (x0,y0) to (x1,y1), as drawn in Golden. */
function Spark({ x0, y0, x1, y1, t }: { x0: number; y0: number; x1: number; y1: number; t: number }) {
  const mx = (x0 + x1) / 2;
  const u = 1 - t;
  const x = u * u * u * x0 + 3 * u * u * t * mx + 3 * u * t * t * mx + t * t * t * x1;
  const y = u * u * u * y0 + 3 * u * u * t * y0 + 3 * u * t * t * y1 + t * t * t * y1;
  return <circle cx={x} cy={y} r={7} fill={c.ink} stroke={c.bottle} strokeWidth={2} style={{ filter: `drop-shadow(0 0 10px ${c.bottle})` }} />;
}

/** 6. One golden image, copy-on-write clones. */
export function Golden() {
  const frame = useCurrentFrame();
  const clones = ["feat/login", "agent/parser", "fix/race", "pr/4053"];
  const ys = [230, 380, 530, 680];
  const card = useIn(6, 18);
  const fan = cue("golden", "each", 0, -4);
  return (
    <Stage>
      <div style={{ position: "absolute", top: 110, left: 230 }}>
        <Kicker delay={2}>How it works</Kicker>
      </div>
      <Glass
        tint="rgba(227,201,94,0.07)"
        style={{
          position: "absolute",
          left: 230,
          top: 340,
          width: 580,
          padding: "36px 42px",
          border: `1px solid ${c.peel}55`,
          boxShadow: `0 40px 120px rgba(0,0,0,0.5), 0 0 90px ${c.peel}22`,
          opacity: card,
          transform: `scale(${0.94 + 0.06 * card})`,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          <Mark size={46} delay={8} />
          <span style={{ fontFamily: serif, fontSize: 48 }}>golden image</span>
        </div>
        <div style={{ fontFamily: mono, fontSize: 19, color: c.muted, marginTop: 22, lineHeight: 1.95, whiteSpace: "nowrap" }}>
          <div>main @ c9483ee</div>
          {(
            [
              ["dependencies · pnpm, uv, cargo", cue("golden", "dependencies")],
              ["dev shell · nix, mise, direnv", cue("golden", "environment")],
              ["local files · .env", cue("golden", "warmed")],
            ] as const
          ).map(([l, at]) => {
            const on = frame >= at;
            return (
              <div key={l} style={{ color: on ? c.ink : c.dim }}>
                <span style={{ color: on ? c.bottle : c.dim }}>{on ? "✓" : "○"}</span> {l}
              </div>
            );
          })}
        </div>
      </Glass>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        <defs>
          <linearGradient id="wire" x1="0" x2="1">
            <stop offset="0" stopColor={c.peel} />
            <stop offset="1" stopColor={c.bottle} />
          </linearGradient>
          <filter id="wireglow" x="-20%" y="-50%" width="140%" height="200%">
            <feGaussianBlur stdDeviation="6" result="b" />
            <feMerge>
              <feMergeNode in="b" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>
        {ys.map((y, i) => {
          const at = fan + i * 6;
          const d = interpolate(frame, [at, at + 16], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
          const y1 = y + 36;
          // Leave the card a little apart from each other, not from one pinch point.
          const y0 = 515 + (i - 1.5) * 18;
          const path = `M 810 ${y0} C 1015 ${y0}, 1015 ${y1}, 1220 ${y1}`;
          const s = (((frame - at - 16) % 36) + 36) % 36 / 36;
          return (
            <g key={i}>
              <path d={path} fill="none" stroke="url(#wire)" strokeWidth={5} strokeLinecap="round" filter="url(#wireglow)" pathLength={1} strokeDasharray={1} strokeDashoffset={1 - d} opacity={d > 0 ? 1 : 0} />
              {d >= 1 && <Spark x0={810} y0={y0} x1={1220} y1={y1} t={s} />}
            </g>
          );
        })}
      </svg>
      {clones.map((l, i) => (
        <CloneCard key={l} label={l} x={1220} y={ys[i]} at={fan + 4 + i * 6} />
      ))}
      <Out at={fan} color={c.bottle} style={{ position: "absolute", left: 1222, top: 172, fontFamily: mono, fontSize: 18, letterSpacing: 4 }}>
        COPY-ON-WRITE CLONES
      </Out>
    </Stage>
  );
}

/** A box in the under-the-hood diagrams. */
function Box({ label, sub, tint, border, at, style }: { label: string; sub?: string; tint: string; border: string; at: number; style?: React.CSSProperties }) {
  const p = useIn(at, 16);
  return (
    <Glass tint={tint} style={{ padding: "18px 24px", border: `1px solid ${border}`, opacity: p, transform: `scale(${0.92 + 0.08 * p})`, ...style }}>
      <div style={{ fontFamily: mono, fontSize: 19, color: c.ink, whiteSpace: "nowrap" }}>{label}</div>
      {sub && <div style={{ fontFamily: mono, fontSize: 14, color: c.muted, marginTop: 6, whiteSpace: "nowrap" }}>{sub}</div>}
    </Glass>
  );
}

function Chip({ children, at, color = c.bottle }: { children: React.ReactNode; at: number; color?: string }) {
  const p = useIn(at, 14);
  return (
    <span
      style={{
        fontFamily: mono,
        fontSize: 18,
        color,
        padding: "6px 14px",
        borderRadius: 999,
        border: `1px solid ${color}55`,
        background: `${color}14`,
        opacity: p,
        display: "inline-block",
        transform: `translateY(${(1 - p) * 8}px)`,
      }}
    >
      {children}
    </span>
  );
}

function Column({ at, os, fs, children }: { at: number; os: string; fs: string; children: React.ReactNode }) {
  const p = useIn(at, 20);
  return (
    <div style={{ width: 700, opacity: p, transform: `translateY(${(1 - p) * 40}px)` }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 18, marginBottom: 26 }}>
        <span style={{ fontFamily: serif, fontSize: 56, letterSpacing: -1 }}>{os}</span>
        <span style={{ fontFamily: mono, fontSize: 20, color: c.bottle, letterSpacing: 2 }}>{fs}</span>
      </div>
      {children}
    </div>
  );
}

/** The copy step between a golden image and its clones, drawn left to right. */
function Link({ label, at }: { label: string; at: number }) {
  const frame = useCurrentFrame();
  const d = interpolate(frame, [at, at + 18], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 14, margin: "10px 4px" }}>
      <div style={{ flex: 1, height: 2, background: `linear-gradient(90deg, ${c.peel}, ${c.bottle})`, transformOrigin: "left", transform: `scaleX(${d})`, opacity: 0.8 }} />
      <span style={{ fontFamily: mono, fontSize: 17, color: c.bottle, opacity: d }}>{label}</span>
    </div>
  );
}

/** One OS: a container holding the golden image and its clones. */
function Pool({ caption, golden, goldenSub, link, cloneSub, at, linkAt, cloneAt }: {
  caption: string; golden: string; goldenSub: string; link: string; cloneSub: string; at: number; linkAt: number; cloneAt: number;
}) {
  const clones = ["feat-login", "fix-race", "agent/parser"];
  return (
    <Glass tint="rgba(233,238,232,0.03)" style={{ padding: "22px 24px" }}>
      <div style={{ fontFamily: mono, fontSize: 18, color: c.dim, marginBottom: 14 }}>{caption}</div>
      <Box label={golden} sub={goldenSub} tint="rgba(227,201,94,0.07)" border={`${c.peel}55`} at={at} />
      <Link label={link} at={linkAt} />
      <div style={{ display: "flex", gap: 12 }}>
        {clones.map((n, i) => (
          <Box key={n} label={n} sub={cloneSub} tint="rgba(124,196,155,0.07)" border={`${c.bottle}66`} at={cloneAt + i * 5} style={{ flex: 1 }} />
        ))}
      </div>
    </Glass>
  );
}

/** 7. Under the hood: APFS clonefile on macOS, btrfs snapshots on Linux. */
export function Internals() {
  const mac = cue("internals", "macos", 0, -6);
  const linux = cue("internals", "linux", 0, -6);
  const snapAt = cue("internals", "snapshot", 0, -4);
  return (
    <Stage push={0.02}>
      <div style={{ position: "absolute", top: 120, left: 0, right: 0, display: "flex", flexDirection: "column", alignItems: "center", gap: 18 }}>
        <Kicker delay={2}>Under the hood</Kicker>
        <Words text="What your file system already does well." size={58} delay={cue("internals", "leans", 0, -4)} accent={["well."]} />
      </div>
      <div style={{ display: "flex", gap: 120, marginTop: 260, alignItems: "flex-start" }}>
        <Column at={mac} os="macOS" fs="APFS">
          <Pool
            caption="disk images · one per clone"
            golden="golden.img"
            goldenSub="the warmed image"
            link="clonefile(2)"
            cloneSub="image clone"
            at={mac + 6}
            linkAt={cue("internals", "copied")}
            cloneAt={cue("internals", "clonefile")}
          />
          <div style={{ display: "flex", gap: 14, marginTop: 24 }}>
            <Chip at={cue("internals", "single", 0, -2)}>one syscall, any repo size</Chip>
            <Chip at={cue("internals", "call", 0, 2)}>mounted in ~0.7s</Chip>
          </div>
        </Column>
        <Column at={linux} os="Linux" fs="btrfs">
          <Pool
            caption="pool.btrfs · mounted once per repo"
            golden=".clove-golden"
            goldenSub="subvolume"
            link="subvolume snapshot"
            cloneSub="snapshot"
            at={linux + 6}
            linkAt={snapAt - 10}
            cloneAt={snapAt}
          />
          <div style={{ display: "flex", gap: 14, marginTop: 24 }}>
            <Chip at={cue("internals", "milliseconds", 0, -4)}>~8 ms per clone</Chip>
            <Chip at={cue("internals", "milliseconds", 0, 6)}>no mount per clone</Chip>
          </div>
        </Column>
      </div>
    </Stage>
  );
}

/** 8. clove new vs git worktree + setup, side by side. */
export function Fast() {
  const frame = useCurrentFrame();
  const race = useRamp(cue("fast", "everything", 0, -6), cue("fast", "disk", 0, 10), Easing.linear);
  const steps: [string, string, boolean][] = [
    ["clone image", "0.00s", true],
    ["mount", "0.70s", true],
    ["checkout", "0.21s", true],
    ["setup direnv", "unchanged", false],
    ["setup pnpm", "unchanged", false],
  ];
  const right = useIn(cue("fast", "everything", 0, -16), 20);
  return (
    <Stage>
      <div style={{ display: "flex", gap: 60, alignItems: "center" }}>
        <Terminal title="~/src/mono" width={820}>
          <Typed text="clove new feat/login" start={6} cps={28} />
          {steps.map(([s, t, ran], i) => (
            <Out key={s} at={cue("fast", "worktree", 0, -2) + i * 3} color={ran ? c.ink : c.dim}>
              {`  ${ran ? "✓" : "·"} ${s.padEnd(14)} ${t}`}
            </Out>
          ))}
          <Out at={cue("fast", "second", 0, -4)} color={c.bottle} style={{ marginTop: 10, fontWeight: 600 }}>
            ● feat-login ready in 1.0s
          </Out>
        </Terminal>
        <Glass style={{ width: 640, padding: "40px 44px", opacity: right, transform: `translateX(${(1 - right) * 50}px)` }}>
          {(
            [
              ["git worktree + setup", 72, "3.1 GB", c.peel],
              ["clove new", 1.0, "6 MB", c.bottle],
            ] as const
          ).map(([label, secs, disk, col]) => {
            const t = Math.min(secs, race * 72);
            const v = t / secs;
            return (
              <div key={label} style={{ marginBottom: 38, fontFamily: mono }}>
                <div style={{ display: "flex", justifyContent: "space-between", fontSize: 22, color: c.muted, marginBottom: 14 }}>
                  <span>{label}</span>
                  <span style={{ color: col }}>
                    {t < 10 ? t.toFixed(1) : Math.round(t)}s · {disk}
                  </span>
                </div>
                <Bar value={v} width={552} height={16} color={col} />
              </div>
            );
          })}
          <div style={{ fontFamily: serif, fontSize: 40, marginTop: 6, opacity: frame > cue("fast", "disk", 0, 12) ? 1 : 0 }}>
            <span style={{ color: c.bottle, fontStyle: "italic" }}>70×</span> faster,{" "}
            <span style={{ color: c.bottle, fontStyle: "italic" }}>500×</span> less disk
          </div>
          <div style={{ fontFamily: mono, fontSize: 20, color: c.muted, marginTop: 18, display: "flex", gap: 28 }}>
            <span style={{ opacity: useIn(cue("fast", "fan", 0, -4), 14) }}>fan · idle</span>
            <span style={{ opacity: useIn(cue("fast", "disk", 0, -4), 14) }}>disk · +6 MB</span>
          </div>
        </Glass>
      </div>
    </Stage>
  );
}

function StepRow({ name, why, runs, i, run }: { name: string; why: string; runs: boolean; i: number; run: number }) {
  const frame = useCurrentFrame();
  const p = useIn(4 + i * 4, 16);
  const lit = runs && frame > cue("steps", "lockfile");
  return (
    <Glass
      tint={lit ? "rgba(124,196,155,0.08)" : "rgba(233,238,232,0.02)"}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 30,
        padding: "24px 32px",
        fontFamily: mono,
        fontSize: 27,
        border: `1px solid ${lit ? `${c.bottle}88` : c.rule}`,
        boxShadow: lit ? `0 0 60px ${c.bottle}22` : "none",
        opacity: p * (runs ? 1 : 0.5),
        transform: `translateX(${(1 - p) * 40}px)`,
      }}
    >
      <span style={{ width: 160, color: lit ? c.ink : c.muted }}>{name}</span>
      <span style={{ flex: 1, color: lit ? c.peel : c.dim }}>{why}</span>
      {runs ? (
        <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
          <Bar value={run} width={230} />
          <span style={{ color: c.bottle, width: 90 }}>{run >= 1 ? "3.8s" : "runs"}</span>
        </div>
      ) : (
        <span style={{ color: c.dim, width: 338, textAlign: "right" }}>skipped</span>
      )}
    </Glass>
  );
}

/** 9. Only what changed runs. */
export function Steps() {
  const run = useRamp(cue("steps", "only"), cue("steps", "again", 0, 8));
  const rows: [string, string, boolean][] = [
    ["mise", "mise.toml unchanged", false],
    ["direnv", "flake.lock unchanged", false],
    ["pnpm", "pnpm-lock.yaml changed", true],
    ["uv", "uv.lock unchanged", false],
    ["cargo", "Cargo.lock unchanged", false],
  ];
  return (
    <Stage>
      <div style={{ display: "flex", flexDirection: "column", gap: 22, width: 1150 }}>
        <Kicker delay={0}>Setup, only where needed</Kicker>
        <div style={{ height: 10 }} />
        {rows.map(([name, why, runs], i) => (
          <StepRow key={name} name={name} why={why} runs={runs} i={i} run={run} />
        ))}
      </div>
    </Stage>
  );
}

/** Refreshes on pull, in the background, while the clones keep working. */
export function Fresh() {
  const frame = useCurrentFrame();
  const k = (w: string, o = 0) => cue("fresh", w, 0, o);
  const term = useIn(4, 18);
  const wire = useRamp(k("background", -6), k("background", 8));
  const panel = useIn(k("background", -2), 18);
  const prog = useRamp(k("refreshes", -4), k("image", 14), Easing.inOut(Easing.quad));
  const done = prog >= 1;
  const right = useIn(k("clones", -10), 18);
  const steps: [string, string, boolean][] = [
    ["pnpm", "unchanged · reused", false],
    ["cargo", "unchanged · reused", false],
    ["codegen", "proto/ changed · rerun", true],
  ];
  const pulse = 0.8 + 0.2 * Math.sin(frame / 7);
  return (
    <Stage push={0.02}>
      <div style={{ position: "absolute", top: 110, left: 150 }}>
        <Kicker delay={2}>Always warm</Kicker>
      </div>
      <div style={{ position: "absolute", left: 150, top: 375, opacity: term, transform: `translateY(${(1 - term) * 30}px)` }}>
        <Terminal title="~/src/mono" width={470}>
          <Typed text="git pull" start={k("pull", -16)} cps={22} />
          <Out at={k("pull", 4)} color={c.muted}>Fast-forward · 12 commits</Out>
        </Terminal>
      </div>
      <div style={{ position: "absolute", left: 640, top: 474, width: 120, height: 2, background: `linear-gradient(90deg, ${c.muted}, ${c.peel})`, transformOrigin: "left", transform: `scaleX(${wire})` }} />
      <div style={{ position: "absolute", left: 640, top: 440, width: 120, textAlign: "center", fontFamily: mono, fontSize: 16, color: c.dim, opacity: wire }}>service</div>
      <Glass
        tint="rgba(227,201,94,0.07)"
        style={{ position: "absolute", left: 780, top: 340, width: 520, padding: "30px 34px", border: `1px solid ${c.peel}55`, opacity: panel, transform: `scale(${0.95 + 0.05 * panel})` }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 20 }}>
          <span style={{ fontFamily: serif, fontSize: 38 }}>golden image</span>
          <span style={{ fontFamily: mono, fontSize: 18, color: done ? c.bottle : c.peel }}>{done ? "gen 15 ✓" : "gen 14 → 15"}</span>
        </div>
        {steps.map(([n, what, ran], i) => {
          const on = prog > (i + 1) / (steps.length + 0.5);
          return (
            <div key={n} style={{ display: "flex", gap: 16, fontFamily: mono, fontSize: 17, lineHeight: 2.1, whiteSpace: "nowrap", color: on ? (ran ? c.peel : c.muted) : c.dim }}>
              <span style={{ width: 22 }}>{on ? (ran ? "↻" : "✓") : "·"}</span>
              <span style={{ width: 100, color: on ? c.ink : c.dim }}>{n}</span>
              <span>{on ? what : ""}</span>
            </div>
          );
        })}
        <div style={{ marginTop: 18, height: 6, borderRadius: 3, background: c.rule }}>
          <div style={{ height: 6, borderRadius: 3, width: `${prog * 100}%`, background: `linear-gradient(90deg, ${c.peel}, ${c.bottle})` }} />
        </div>
      </Glass>
      <div style={{ position: "absolute", left: 1400, top: 352, display: "flex", flexDirection: "column", gap: 16, opacity: 0.35 + 0.65 * right }}>
        {["feat/login", "agent/parser", "fix/race"].map((n) => (
          <Glass key={n} style={{ width: 360, padding: "20px 24px", fontFamily: mono, fontSize: 18, display: "flex", justifyContent: "space-between", alignItems: "center", border: `1px solid ${right > 0.5 ? `${c.bottle}55` : c.rule}` }}>
            <span>{n}</span>
            <span style={{ fontSize: 14, color: c.bottle, opacity: right > 0.5 ? pulse : 0.4, whiteSpace: "nowrap" }}>● working</span>
          </Glass>
        ))}
      </div>
      <div style={{ position: "absolute", top: 820, left: 0, right: 0, display: "flex", justifyContent: "center", gap: 16 }}>
        <Chip at={k("pull", 2)}>on every fetch or pull</Chip>
        <Chip at={k("refreshes", 12)}>only changed steps rerun</Chip>
        <Chip at={k("clones", -2)}>clones never touched</Chip>
      </div>
    </Stage>
  );
}

/** A detected tool, lighting up as the voice names it. */
function Tool({ name, kind, at }: { name: string; kind: string; at: number }) {
  const frame = useCurrentFrame();
  const p = useIn(at, 14);
  const lit = frame >= at;
  return (
    <Glass
      tint={lit ? "rgba(124,196,155,0.08)" : "rgba(233,238,232,0.02)"}
      style={{
        padding: "18px 22px",
        fontFamily: mono,
        border: `1px solid ${lit ? `${c.bottle}77` : c.rule}`,
        boxShadow: lit ? `0 0 40px ${c.bottle}1f` : "none",
        opacity: 0.35 + 0.65 * p,
        transform: `scale(${0.96 + 0.04 * p})`,
      }}
    >
      <div style={{ fontSize: 22, color: lit ? c.ink : c.dim, whiteSpace: "nowrap" }}>
        <span style={{ color: lit ? c.bottle : c.dim }}>{lit ? "✓" : "·"}</span> {name}
      </div>
      <div style={{ fontSize: 13, color: c.dim, marginTop: 4, whiteSpace: "nowrap" }}>{kind}</div>
    </Glass>
  );
}

/** 13. Detected on its own; anything else is a config step. */
export function Detect() {
  const k = (w: string, o = -3) => cue("detect", w, 0, o);
  const tools: [string, string, number][] = [
    ["mise", "tool versions", k("mise")],
    ["direnv", "environment", k("direnv")],
    ["nix", "dev shell", k("nix")],
    ["pnpm", "node", k("pnpm")],
    ["bun", "node", k("bun")],
    ["yarn", "node", k("yarn")],
    ["npm", "node", k("npm")],
    ["uv", "python", k("uv")],
    ["cargo", "rust", k("cargo")],
    [".env", "local files", k("local")],
  ];
  const cfg = useIn(k("anything", -6), 20);
  const conf = k("config", -2);
  return (
    <Stage push={0.02}>
      <div style={{ position: "absolute", top: 200, left: 0, right: 0, display: "flex", flexDirection: "column", alignItems: "center", gap: 16 }}>
        <Kicker delay={2}>Detected, then configurable</Kicker>
        <Words text="Your toolchain, found on its own." size={60} delay={k("detects", -6)} accent={["own."]} />
      </div>
      <div style={{ display: "flex", gap: 70, alignItems: "center", marginTop: 170 }}>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 190px)", gap: 16 }}>
          {tools.map(([n, kind, at]) => (
            <Tool key={n} name={n} kind={kind} at={at} />
          ))}
        </div>
        <div style={{ opacity: cfg, transform: `translateX(${(1 - cfg) * 40}px)` }}>
          <Terminal title=".clove.toml · shared" width={560}>
            <div style={{ fontSize: 22, lineHeight: 1.7 }}>
              <Out at={conf} color={c.muted}>{"[[add_step]]"}</Out>
              <Out at={conf + 4} color={c.ink}>{'name  = "codegen"'}</Out>
              <Out at={conf + 8} color={c.ink}>{'warm  = "make proto"'}</Out>
              <Out at={conf + 12} color={c.ink}>{'setup = "make proto"'}</Out>
              <Out at={conf + 16} color={c.peel}>{'watch = ["proto/"]'}</Out>
            </div>
          </Terminal>
        </div>
      </div>
    </Stage>
  );
}

/* --------------------------------------------------------------- surfaces */

type Cell = { t: string; fg: string | null; bg: string | null; b: boolean; i: boolean; d: boolean };
type Grid = { width: number; height: number; rows: Cell[][] };

/** A TUI screen exported from clove-tui's test harness, drawn cell by cell. */
function TuiShot({ name, fontSize = 15 }: { name: string; fontSize?: number }) {
  const [grid, setGrid] = useState<Grid | null>(null);
  const [handle] = useState(() => (has(`shots/${name}.json`) ? delayRender(`tui ${name}`) : null));
  useEffect(() => {
    if (handle === null) return;
    fetch(staticFile(`shots/${name}.json`))
      .then((r) => r.json())
      .then((g: Grid) => setGrid(g))
      .catch(() => setGrid(null))
      .finally(() => continueRender(handle));
  }, [handle, name]);
  if (!grid) return <Placeholder label={`TUI · ${name}`} w={1000} h={560} />;
  // A fixed cell grid: the web font lacks box drawing and block glyphs, and a
  // fallback font's widths would drift, so lines and bars are drawn as shapes.
  // Whole pixels, so neighbouring backgrounds and bars meet without seams.
  const cw = Math.round(fontSize * 0.6);
  const ch = Math.round(fontSize * 1.32);
  return (
    <div style={{ fontFamily: mono, fontSize, background: "#0b100d", padding: 18 }}>
      {grid.rows.map((row, y) => (
        <div key={y} style={{ display: "flex", height: ch }}>
          {row.map((cell, x) => (
            <TuiCell key={x} cell={cell} w={cw} h={ch} />
          ))}
        </div>
      ))}
    </div>
  );
}

const BLOCKS: Record<string, number> = { "█": 1, "▉": 7 / 8, "▊": 3 / 4, "▋": 5 / 8, "▌": 1 / 2, "▍": 3 / 8, "▎": 1 / 4, "▏": 1 / 8 };
const CORNERS: Record<string, [boolean, boolean]> = { "╭": [false, false], "╮": [true, false], "╰": [false, true], "╯": [true, true] };

function TuiCell({ cell, w, h }: { cell: Cell; w: number; h: number }) {
  const fg = cell.fg ?? c.ink;
  const lw = Math.max(1, w / 7);
  // Coloured cells bleed 1px right, so the camera's zoom can't open seams between them.
  const box: React.CSSProperties = { position: "relative", width: w, height: h, flex: "none", boxShadow: cell.bg ? `1px 0 0 ${cell.bg}` : undefined, background: cell.bg ?? undefined, opacity: cell.d ? 0.6 : 1 };
  const t = cell.t;
  const line = (st: React.CSSProperties) => <div style={{ position: "absolute", background: fg, ...st }} />;
  if (t === "─") return <div style={box}>{line({ left: 0, right: -1, top: (h - lw) / 2, height: lw })}</div>;
  if (t === "│") return <div style={box}>{line({ top: 0, bottom: -1, left: (w - lw) / 2, width: lw })}</div>;
  if (t in CORNERS) {
    const [left, up] = CORNERS[t];
    const r = Math.min(w, h) / 2;
    const st: React.CSSProperties = {
      position: "absolute",
      boxSizing: "border-box",
      width: w / 2 + lw / 2,
      height: h / 2 + lw / 2,
      [left ? "right" : "left"]: (w - lw) / 2,
      [up ? "bottom" : "top"]: (h - lw) / 2,
      [left ? "borderRight" : "borderLeft"]: `${lw}px solid ${fg}`,
      [up ? "borderBottom" : "borderTop"]: `${lw}px solid ${fg}`,
      [`border${up ? "Bottom" : "Top"}${left ? "Right" : "Left"}Radius`]: r,
    };
    return (
      <div style={box}>
        <div style={st} />
      </div>
    );
  }
  if (t in BLOCKS) return <div style={box}>{line({ left: 0, top: 0, bottom: 0, width: w * BLOCKS[t] + (BLOCKS[t] === 1 ? 1 : 0) })}</div>;
  return (
    <div
      style={{
        ...box,
        color: fg,
        lineHeight: `${h}px`,
        textAlign: "center",
        fontWeight: cell.b ? 600 : 400,
        fontStyle: cell.i ? "italic" : undefined,
        whiteSpace: "pre",
        overflow: "visible",
      }}
    >
      {t}
    </div>
  );
}

function AppShot({ name, width }: { name: string; width: number }) {
  if (!has(`shots/${name}.png`)) return <Placeholder label={`app · ${name}`} w={width} h={width * 0.625} />;
  return <Img src={staticFile(`shots/${name}.png`)} style={{ width, display: "block" }} />;
}

function Placeholder({ label, w, h }: { label: string; w: number; h: number }) {
  return (
    <div
      style={{
        width: w,
        height: h,
        border: `1px dashed ${c.dim}`,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontFamily: mono,
        color: c.dim,
        fontSize: 22,
      }}
    >
      {label}
    </div>
  );
}

function Window({ children, title }: { children: React.ReactNode; title?: string }) {
  return (
    <div style={{ borderRadius: 16, overflow: "hidden", border: `1px solid ${c.rule}`, boxShadow: "0 60px 160px rgba(0,0,0,0.65)", background: "#0b100d" }}>
      {title && (
        <div style={{ display: "flex", alignItems: "center", gap: 9, padding: "12px 16px", borderBottom: `1px solid ${c.rule}` }}>
          {["#ec6a5e", "#f4bf4f", "#61c554"].map((bg) => (
            <div key={bg} style={{ width: 12, height: 12, borderRadius: 6, background: bg, opacity: 0.4 }} />
          ))}
          <span style={{ flex: 1, textAlign: "center", fontFamily: mono, fontSize: 15, color: c.dim, marginRight: 50 }}>{title}</span>
        </div>
      )}
      {children}
    </div>
  );
}

/** One of the three ways in: arrives large, then steps back as the next comes. */
function Layer({ at, next, label, children }: { at: number; next?: number; label: string; children: React.ReactNode }) {
  const frame = useCurrentFrame();
  const p = useIn(at, 22);
  const back = next === undefined ? 0 : interpolate(frame, [next, next + 16], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) });
  if (frame < at - 1) return null;
  return (
    <div
      style={{
        position: "absolute",
        left: "50%",
        top: "53%",
        opacity: Math.min(1, p * 1.3) * (1 - back * 0.9),
        transform: `translate(-50%, -50%) translateY(${(1 - p) * 70 - back * 150}px) scale(${(0.94 + 0.06 * p) * (1 - back * 0.14)})`,
        filter: `blur(${back * 4}px)`,
      }}
    >
      <div style={{ display: "flex", justifyContent: "center", marginBottom: 22, opacity: 1 - back }}>
        <Kicker delay={at}>{label}</Kicker>
      </div>
      {children}
    </div>
  );
}

/** 10. Command line, terminal UI, desktop app; one at a time, with the voice. */
export function Surfaces() {
  const tuiAt = cue("surfaces", "full-screen", 0, -6);
  const appAt = cue("surfaces", "desktop", 0, -8);
  return (
    <Stage push={0.02}>
      <Layer at={4} next={tuiAt} label="The command line">
        <Terminal title="~/src/mono" width={1000}>
          <Typed text="clove ls" start={12} cps={24} />
          <Out at={30}>{"  NAME           CHANGES   OWN"}</Out>
          <Out at={33} color={c.ink}>{"● feat-login     ↑2        14M"}</Out>
          <Out at={36} color={c.ink}>{"● fix-race       ↑1 ↓3    230M"}</Out>
          <Out at={39}>{"○ spike-cache    unmounted"}</Out>
        </Terminal>
      </Layer>
      <Layer at={tuiAt} next={appAt} label="A full-screen terminal UI">
        <Window>
          <TuiShot name="tui-main" fontSize={17.5} />
        </Window>
      </Layer>
      <Layer at={appAt} label="The desktop app · macOS and Linux">
        <Window title="clove">
          <AppShot name="app-repo" width={1240} />
        </Window>
      </Layer>
    </Stage>
  );
}

/* --------------------------------------------------------------- features */

function Feature({
  at,
  until,
  kicker,
  children,
  shot,
}: {
  at: number;
  until?: number;
  kicker: string;
  children: React.ReactNode;
  shot: React.ReactNode;
}) {
  const frame = useCurrentFrame();
  const p = useIn(at, 20);
  const out = until === undefined ? 0 : interpolate(frame, [until - 9, until], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  if (frame < at - 1 || out >= 1) return null;
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 80,
        opacity: p * (1 - out),
        transform: `translateY(${(1 - p) * 40 - out * 30}px)`,
      }}
    >
      <div style={{ width: 560 }}>
        <Kicker delay={at}>{kicker}</Kicker>
        <div style={{ height: 24 }} />
        {children}
      </div>
      {shot}
    </div>
  );
}

/** 11. Update with your agent, prune, agents; timed to the three sentences. */
export function Features() {
  const a = 0;
  const b = cue("features", "cleans", 0, -10);
  const d = cue("features", "teaches", 0, -10);
  return (
    <Stage push={0.015}>
      <Feature
        at={a}
        until={b}
        kicker="clove update"
        shot={
          <Window>
            <TuiShot name="tui-conflict" fontSize={13} />
          </Window>
        }
      >
        <Words text="Up to date, with your agent resolving the conflicts." size={60} delay={a + 4} accent={["agent"]} />
      </Feature>
      <Feature
        at={b}
        until={d}
        kicker="clove prune · clove log"
        shot={
          <Window title="clove">
            <AppShot name="app-history" width={820} />
          </Window>
        }
      >
        <Words text="Merged pull requests clean up after themselves." size={60} delay={b + 4} accent={["themselves."]} />
      </Feature>
      <Feature
        at={d}
        kicker="clove agents install"
        shot={
          <Terminal title="~/src/mono" width={720}>
            <Typed text="clove agents install" start={d + 8} cps={28} />
            <Out at={d + 30} color={c.bottle}>
              ✓ Claude Code skill
            </Out>
            <Out at={d + 34} color={c.bottle}>
              ✓ ~/.claude/CLAUDE.md
            </Out>
            <Out at={d + 38} color={c.bottle}>
              ✓ worktree hooks
            </Out>
            <Out at={d + 42} color={c.bottle}>
              ✓ Codex AGENTS.md
            </Out>
          </Terminal>
        }
      >
        <Words text="And your agents reach for clove on their own." size={60} delay={d + 4} accent={["agents"]} />
      </Feature>
    </Stage>
  );
}

/** 12. Install. */
function Letter({ ch, at }: { ch: string; at: number }) {
  const p = useIn(at, 16);
  return <span style={{ display: "inline-block", opacity: p, transform: `translateY(${(1 - p) * 24}px)` }}>{ch}</span>;
}

export function Outro() {
  const tag = useIn(cue("outro", "worktrees", 0, -4));
  const cmd = useIn(cue("outro", "second", 0, 8));
  // The domain lands as the voice says it.
  const site = useIn(cue("outro", "clove", 1, -4), 14);
  return (
    <Stage push={0.03}>
      <div
        style={{
          position: "absolute",
          width: 1000,
          height: 1000,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(124,196,155,0.18), transparent 60%)",
        }}
      />
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 30 }}>
          <Mark size={170} delay={14} alive />
          <div style={{ fontFamily: serif, fontSize: 150, letterSpacing: -5, display: "flex" }}>
            {"clove".split("").map((ch, i) => (
              <Letter key={i} ch={ch} at={24 + i * 3} />
            ))}
          </div>
        </div>
        <div
          style={{
            fontFamily: serif,
            fontSize: 52,
            color: c.muted,
            marginTop: 8,
            opacity: tag,
            transform: `translateY(${(1 - tag) * 12}px)`,
          }}
        >
          Worktrees, <span style={{ color: c.bottle, fontStyle: "italic" }}>ready in a second.</span>
        </div>
        <Glass style={{ marginTop: 70, fontFamily: mono, fontSize: 30, padding: "22px 36px", opacity: cmd, transform: `translateY(${(1 - cmd) * 16}px)` }}>
          <span style={{ color: c.bottle }}>❯ </span>brew install pierre-borckmans/tap/clove
        </Glass>
        <div style={{ display: "flex", gap: 14, marginTop: 28, opacity: cmd }}>
          {["macOS", "Linux"].map((os) => (
            <span key={os} style={{ fontFamily: mono, fontSize: 19, color: c.muted, padding: "6px 16px", borderRadius: 999, border: `1px solid ${c.rule}` }}>
              {os}
            </span>
          ))}
        </div>
        <div style={{ fontFamily: mono, fontSize: 34, color: c.bottle, marginTop: 30, letterSpacing: 1, opacity: site, transform: `scale(${0.9 + 0.1 * site})`, textShadow: `0 0 ${24 + 20 * site}px ${c.bottle}66` }}>clove.fast</div>
        <div style={{ fontFamily: mono, fontSize: 19, color: c.dim, marginTop: 10, opacity: cmd, display: "flex", alignItems: "center", gap: 10 }}>
          <svg width={20} height={20} viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
          </svg>
          github.com/pierre-borckmans/clove
        </div>
      </div>
    </Stage>
  );
}
