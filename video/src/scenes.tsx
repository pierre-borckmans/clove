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
import { Bar, c, Glass, Kicker, Mark, mono, Out, sec, serif, Stage, Terminal, Typed, useIn, useRamp, Words } from "./ui";

const has = (name: string) => getStaticFiles().some((f) => f.name === name);

/* ---------------------------------------------------------------- problem */

/** 1. git gives you a worktree in a second. */
export function Worktree() {
  return (
    <Stage>
      <div style={{ position: "absolute", top: 150, left: 230 }}>
        <Kicker delay={2}>A new task</Kicker>
      </div>
      <Terminal title="~/src/mono" width={1180} delay={4} style={{ marginTop: 40 }}>
        <Typed text="git worktree add ../feat-login -b feat/login" start={12} />
        <Out at={sec(1.85)}>Preparing worktree (new branch 'feat/login')</Out>
        <Out at={sec(2.05)}>HEAD is now at c9483ee feat(api): rate limits per team</Out>
        <Out at={sec(2.4)} color={c.bottle}>
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
      <div style={{ fontFamily: mono, fontSize: 18, color: c.dim, marginTop: 8, marginLeft: 44 }}>{hint}</div>
    </Glass>
  );
}

/** 2. ...but empty. */
export function Empty() {
  return (
    <Stage>
      <div style={{ display: "flex", gap: 56, alignItems: "center" }}>
        <div>
          <Kicker delay={0} color={c.red}>
            But
          </Kicker>
          <div style={{ height: 28 }} />
          <Terminal title="~/src/feat-login" width={900} glow={c.red}>
            <Typed text="pnpm dev" start={8} />
            <Out at={sec(0.85)} color={c.red}>
              sh: next: command not found
            </Out>
            <Out at={sec(1.05)} color={c.dim}>
              {"✗ node_modules missing"}
            </Out>
          </Terminal>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 20, marginTop: 50 }}>
          <Missing label="node_modules" hint="4,470 packages to install" at={sec(1.3)} />
          <Missing label="dev shell" hint="nix flake to evaluate" at={sec(2.0)} />
          <Missing label=".env" hint="local files git never had" at={sec(2.9)} />
        </div>
      </div>
    </Stage>
  );
}

function Ring({ value, size = 360, stroke = 14, color = c.peel }: { value: number; size?: number; stroke?: number; color?: string }) {
  const r = (size - stroke) / 2;
  const len = 2 * Math.PI * r;
  return (
    <svg width={size} height={size} style={{ position: "absolute", inset: 0, transform: "rotate(-90deg)" }}>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="rgba(233,238,232,0.07)" strokeWidth={stroke} />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke={color}
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={len}
        strokeDashoffset={len * (1 - value)}
        style={{ filter: `drop-shadow(0 0 12px ${color}88)` }}
      />
    </svg>
  );
}

/** 3. Getting ready: over a minute, gigabytes, every time. */
export function Cost() {
  const { durationInFrames } = useVideoConfig();
  const k = useRamp(10, durationInFrames - 34, Easing.inOut(Easing.quad));
  const t = k * 72;
  const rows: [string, number, number][] = [
    ["direnv · evaluating the nix flake", 0, 41],
    ["pnpm install --offline", 41, 71],
    ["copy .env and local files", 71, 72],
  ];
  return (
    <Stage>
      <div style={{ position: "absolute", top: 130, left: 230 }}>
        <Kicker delay={2} color={c.peel}>
          Every new worktree
        </Kicker>
      </div>
      <div style={{ display: "flex", gap: 130, alignItems: "center", marginTop: 40 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 40, fontFamily: mono, fontSize: 25 }}>
          {rows.map(([label, from, to]) => {
            const v = (t - from) / (to - from);
            const active = v > 0 && v < 1;
            return (
              <div key={label} style={{ opacity: t >= from - 0.5 ? 1 : 0.35 }}>
                <div style={{ display: "flex", justifyContent: "space-between", color: active ? c.ink : c.muted, marginBottom: 14 }}>
                  <span>{label}</span>
                  <span style={{ color: c.dim }}>{Math.round(Math.max(0, Math.min(t, to) - from))}s</span>
                </div>
                <Bar value={v} width={660} color={v >= 1 ? c.dim : c.peel} />
              </div>
            );
          })}
        </div>
        <div
          style={{
            position: "relative",
            width: 360,
            height: 360,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexDirection: "column",
          }}
        >
          <Ring value={k} />
          <div style={{ fontFamily: serif, fontSize: 132, letterSpacing: -4, lineHeight: 1 }}>{Math.round(t)}s</div>
          <div style={{ fontFamily: mono, fontSize: 28, color: c.peel, marginTop: 12 }}>+{(k * 3.1).toFixed(1)} GB</div>
        </div>
      </div>
    </Stage>
  );
}

function AgentCard({ name, i }: { name: string; i: number }) {
  const frame = useCurrentFrame();
  const p = useIn(i * 4, 16);
  const waited = Math.min(72, Math.max(0, Math.floor((frame - i * 4) / 1.3)));
  const spin = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"[Math.floor(frame / 3 + i) % 10];
  return (
    <Glass style={{ width: 300, padding: "26px 26px 22px", fontFamily: mono, opacity: p, transform: `translateY(${(1 - p) * 30}px)` }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, fontSize: 22 }}>
        <span style={{ width: 9, height: 9, borderRadius: 5, background: c.peel, boxShadow: `0 0 10px ${c.peel}` }} />
        {name}
      </div>
      <div style={{ fontSize: 18, color: c.peel, marginTop: 18 }}>
        {spin} setting up · {waited}s
      </div>
      <div style={{ marginTop: 14 }}>
        <Bar value={waited / 72} width={248} height={6} color={c.peel} />
      </div>
    </Glass>
  );
}

/** 4. Agents multiply it. */
export function Agents() {
  const names = ["agent/parser", "agent/cache", "fix/race", "agent/docs", "agent/tests"];
  const total = useRamp(sec(1.2), sec(5.2));
  return (
    <Stage>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 70 }}>
        <div style={{ display: "flex", gap: 26 }}>
          {names.map((n, i) => (
            <AgentCard key={n} name={n} i={i} />
          ))}
        </div>
        <div style={{ textAlign: "center", opacity: total }}>
          <div style={{ fontFamily: serif, fontSize: 96, letterSpacing: -3 }}>
            {Math.round(total * 6)} <span style={{ color: c.muted }}>minutes</span>
          </div>
          <div style={{ fontFamily: mono, fontSize: 22, color: c.dim, marginTop: 6 }}>of waiting, before anyone writes a line</div>
        </div>
      </div>
    </Stage>
  );
}

/* --------------------------------------------------------------- solution */

/** 5. The mark. */
export function Logo() {
  const word = useIn(16, 200);
  const glow = useRamp(10, 40);
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
      <div style={{ display: "flex", alignItems: "center", gap: 40 }}>
        <Mark size={260} delay={2} />
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
  return <circle cx={x} cy={y} r={5} fill={c.bottle} style={{ filter: `drop-shadow(0 0 8px ${c.bottle})` }} />;
}

/** 6. One golden image, copy-on-write clones. */
export function Golden() {
  const frame = useCurrentFrame();
  const clones = ["feat/login", "agent/parser", "fix/race", "pr/4053"];
  const ys = [230, 380, 530, 680];
  const card = useIn(6, 18);
  const fan = sec(5.4);
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
          width: 560,
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
        <div style={{ fontFamily: mono, fontSize: 23, color: c.muted, marginTop: 22, lineHeight: 1.9 }}>
          <div>main @ c9483ee</div>
          {(
            [
              ["node_modules installed", sec(2.4)],
              ["dev shell warmed", sec(3.0)],
              [".env and local files", sec(3.5)],
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
            <stop offset="0" stopColor={c.peel} stopOpacity="0.7" />
            <stop offset="1" stopColor={c.bottle} stopOpacity="0.7" />
          </linearGradient>
        </defs>
        {ys.map((y, i) => {
          const at = fan + i * 6;
          const d = interpolate(frame, [at, at + 16], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
          const y1 = y + 36;
          const path = `M 790 515 C 1005 515, 1005 ${y1}, 1220 ${y1}`;
          const s = (((frame - at - 16) % 36) + 36) % 36 / 36;
          return (
            <g key={i}>
              <path d={path} fill="none" stroke="url(#wire)" strokeWidth={2.5} strokeDasharray={480} strokeDashoffset={480 * (1 - d)} />
              {d >= 1 && <Spark x0={790} y0={515} x1={1220} y1={y1} t={s} />}
            </g>
          );
        })}
      </svg>
      {clones.map((l, i) => (
        <CloneCard key={l} label={l} x={1220} y={ys[i]} at={fan + 4 + i * 6} />
      ))}
      <Out at={fan + 10} color={c.bottle} style={{ position: "absolute", left: 870, top: 440, fontFamily: mono, fontSize: 19, letterSpacing: 2 }}>
        copy-on-write
      </Out>
    </Stage>
  );
}

/** 7. clove new vs git worktree + setup, side by side. */
export function Fast() {
  const frame = useCurrentFrame();
  const race = useRamp(sec(2.6), sec(5.2), Easing.linear);
  const steps: [string, string, boolean][] = [
    ["clone image", "0.00s", true],
    ["mount", "0.70s", true],
    ["checkout", "0.21s", true],
    ["setup direnv", "unchanged", false],
    ["setup pnpm", "unchanged", false],
  ];
  const right = useIn(sec(2.3), 20);
  return (
    <Stage>
      <div style={{ display: "flex", gap: 60, alignItems: "center" }}>
        <Terminal title="~/src/mono" width={820}>
          <Typed text="clove new feat/login" start={6} cps={28} />
          {steps.map(([s, t, ran], i) => (
            <Out key={s} at={sec(0.9) + i * 3} color={ran ? c.ink : c.dim}>
              {`  ${ran ? "✓" : "·"} ${s.padEnd(14)} ${t}`}
            </Out>
          ))}
          <Out at={sec(1.6)} color={c.bottle} style={{ marginTop: 10, fontWeight: 600 }}>
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
          <div style={{ fontFamily: serif, fontSize: 40, marginTop: 6, opacity: frame > sec(5.3) ? 1 : 0 }}>
            <span style={{ color: c.bottle, fontStyle: "italic" }}>70×</span> faster,{" "}
            <span style={{ color: c.bottle, fontStyle: "italic" }}>500×</span> less disk
          </div>
        </Glass>
      </div>
    </Stage>
  );
}

function StepRow({ name, why, runs, i, run }: { name: string; why: string; runs: boolean; i: number; run: number }) {
  const frame = useCurrentFrame();
  const p = useIn(4 + i * 4, 16);
  const lit = runs && frame > sec(0.9);
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

/** 8. Only what changed runs. */
export function Steps() {
  const run = useRamp(sec(1.3), sec(2.6));
  const rows: [string, string, boolean][] = [
    ["mise", "mise.toml unchanged", false],
    ["direnv", "flake.lock unchanged", false],
    ["pnpm", "pnpm-lock.yaml changed", true],
    ["codegen", "proto/ unchanged", false],
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
  return (
    <div style={{ fontFamily: mono, fontSize, lineHeight: 1.32, background: "#0b100d", padding: 18, whiteSpace: "pre" }}>
      {grid.rows.map((row, y) => (
        <div key={y}>
          {row.map((cell, x) => (
            <span
              key={x}
              style={{
                color: cell.fg ?? c.ink,
                background: cell.bg ?? undefined,
                fontWeight: cell.b ? 600 : 400,
                fontStyle: cell.i ? "italic" : undefined,
                opacity: cell.d ? 0.6 : 1,
              }}
            >
              {cell.t}
            </span>
          ))}
        </div>
      ))}
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

/** 9. Command line, terminal UI, Mac app. */
export function Surfaces() {
  const frame = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const cli = useIn(4, 20);
  const tui = useIn(sec(1.5), 20);
  const app = useIn(sec(2.6), 20);
  const drift = interpolate(frame, [0, durationInFrames], [0, -30]);
  return (
    <Stage push={0.02}>
      <div style={{ position: "absolute", top: 90, left: 200 }}>
        <Kicker delay={0}>Three ways in</Kicker>
      </div>
      <div style={{ position: "absolute", left: 150, top: 230, opacity: cli, transform: `translateY(${(1 - cli) * 50 + drift * 0.4}px) rotate(-1.2deg)` }}>
        <Terminal title="clove" width={640}>
          <Typed text="clove ls" start={10} cps={24} />
          <Out at={30}>{"● feat-login   ↑2      14M"}</Out>
          <Out at={33}>{"● fix-race     ↑1 ↓3  230M"}</Out>
          <Out at={36}>{"○ spike-cache  unmounted"}</Out>
        </Terminal>
      </div>
      <div
        style={{
          position: "absolute",
          left: 560,
          top: 300,
          opacity: tui,
          transform: `translateY(${(1 - tui) * 60 + drift * 0.7}px) scale(0.72) rotate(0.6deg)`,
          transformOrigin: "top left",
        }}
      >
        <Window>
          <TuiShot name="tui-main" />
        </Window>
      </div>
      <div style={{ position: "absolute", right: 120, top: 360, opacity: app, transform: `translateY(${(1 - app) * 70 + drift}px) rotate(1.4deg)` }}>
        <Window title="clove">
          <AppShot name="app-repo" width={860} />
        </Window>
      </div>
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

/** 10. Update with Claude, prune, agents; timed to the three sentences. */
export function Features() {
  const a = 0;
  const b = sec(2.7);
  const d = sec(5.0);
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
        <Words text="Up to date, with Claude resolving the conflicts." size={60} delay={a + 4} accent={["Claude"]} />
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
          <Terminal title="~" width={720}>
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

/** 11. Install. */
export function Outro() {
  const tag = useIn(sec(1.1));
  const cmd = useIn(sec(1.8));
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
          <Mark size={170} delay={0} />
          <div style={{ fontFamily: serif, fontSize: 150, letterSpacing: -5 }}>clove</div>
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
        <div style={{ fontFamily: mono, fontSize: 22, color: c.dim, marginTop: 26, opacity: cmd }}>github.com/pierre-borckmans/clove</div>
      </div>
    </Stage>
  );
}
