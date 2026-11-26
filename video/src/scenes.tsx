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

function Session({ tool, branch, x, y, at }: { tool: string; branch: string; x: number; y: number; at: number }) {
  const frame = useCurrentFrame();
  const p = useIn(at, 12);
  const bob = Math.sin((frame + at) / 18) * 4;
  const spin = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"[Math.floor(frame / 3 + at) % 10];
  return (
    <Glass
      style={{
        position: "absolute",
        left: x,
        top: y + bob,
        width: 330,
        padding: "18px 22px",
        fontFamily: mono,
        opacity: p,
        transform: `scale(${0.7 + 0.3 * p})`,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 20 }}>
        <span style={{ color: c.bottle }}>{tool}</span>
        <span style={{ color: c.peel }}>{spin}</span>
      </div>
      <div style={{ fontSize: 22, marginTop: 8 }}>{branch}</div>
      <div style={{ fontSize: 16, color: c.dim, marginTop: 6 }}>thinking…</div>
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
  const branches: [string, string, number, number][] = [
    ["feat/login", "claude", 420, 330],
    ["fix/race", "codex", 640, 760],
    ["agent/parser", "claude", 860, 300],
    ["agent/docs", "cursor", 1080, 780],
  ];
  return (
    <Stage push={0.025}>
      <div style={{ position: "absolute", top: 110, left: 0, right: 0, display: "flex", justifyContent: "center" }}>
        <Words text="In parallel, each on its own branch." size={64} delay={cue("parallel", "need", 0, -6)} accent={["parallel,"]} />
      </div>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        <line x1={200} y1={540} x2={200 + 1520 * main} y2={540} stroke={c.ink} strokeOpacity={0.5} strokeWidth={4} strokeLinecap="round" />
        {branches.map(([, , x, y], i) => {
          const d = interpolate(frame, [fork + i * 5, fork + i * 5 + 16], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
          const path = `M ${x} 540 C ${x + 120} 540, ${x + 80} ${y}, ${x + 260} ${y} L ${x + 620} ${y}`;
          return (
            <g key={i}>
              <path d={path} fill="none" stroke={c.bottle} strokeWidth={3} strokeDasharray={900} strokeDashoffset={900 * (1 - d)} strokeLinecap="round" />
              <circle cx={x} cy={540} r={9} fill={c.paper} stroke={c.ink} strokeWidth={3} opacity={d > 0 ? 1 : 0} />
            </g>
          );
        })}
        <text x={180} y={590} fill={c.muted} fontFamily={mono} fontSize={22} opacity={main}>
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
            <Out at={cue("empty", "empty")} color={c.red}>
              sh: next: command not found
            </Out>
            <Out at={cue("empty", "empty", 0, 6)} color={c.dim}>
              {"✗ node_modules missing"}
            </Out>
          </Terminal>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 20, marginTop: 50 }}>
          <Missing label="node_modules" hint="4,470 packages to install" at={cue("empty", "dependencies", 0, -4)} />
          <Missing label="dev shell" hint="nix flake to evaluate" at={cue("empty", "dev", 0, -4)} />
          <Missing label=".env" hint="local files git never had" at={cue("empty", "local", 0, -4)} />
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
  const durationInFrames = useSceneLength();
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
  const steps = ["pnpm install", "nix develop", "make proto", "ln -s ../mono/…"];
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
  const at = cue("logo", "this", 0, -6);
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
  return <circle cx={x} cy={y} r={5} fill={c.bottle} style={{ filter: `drop-shadow(0 0 8px ${c.bottle})` }} />;
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
              ["node_modules installed", cue("golden", "dependencies")],
              ["dev shell warmed", cue("golden", "environment")],
              [".env and local files", cue("golden", "warmed")],
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

/** A box in the under-the-hood diagrams. */
function Box({ label, sub, tint, border, at, style }: { label: string; sub?: string; tint: string; border: string; at: number; style?: React.CSSProperties }) {
  const p = useIn(at, 16);
  return (
    <Glass tint={tint} style={{ padding: "18px 24px", border: `1px solid ${border}`, opacity: p, transform: `scale(${0.92 + 0.08 * p})`, ...style }}>
      <div style={{ fontFamily: mono, fontSize: 24, color: c.ink }}>{label}</div>
      {sub && <div style={{ fontFamily: mono, fontSize: 17, color: c.muted, marginTop: 6 }}>{sub}</div>}
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

/** 7. Under the hood: APFS clonefile on macOS, btrfs snapshots on Linux. */
export function Internals() {
  const frame = useCurrentFrame();
  const mac = cue("internals", "macos", 0, -6);
  const linux = cue("internals", "linux", 0, -6);
  const arrow = interpolate(frame, [cue("internals", "copied"), cue("internals", "clonefile", 0, 8)], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const snaps = ["feat-login", "fix-race", "agent/parser"];
  const snapAt = cue("internals", "snapshot", 0, -4);
  return (
    <Stage push={0.02}>
      <div style={{ position: "absolute", top: 120, left: 0, right: 0, display: "flex", flexDirection: "column", alignItems: "center", gap: 18 }}>
        <Kicker delay={2}>Under the hood</Kicker>
        <Words text="What your file system already does well." size={58} delay={cue("internals", "leans", 0, -4)} accent={["well."]} />
      </div>
      <div style={{ display: "flex", gap: 120, marginTop: 260, alignItems: "flex-start" }}>
        <Column at={mac} os="macOS" fs="APFS">
          <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
            <Box label="golden.img" sub="the warmed image" tint="rgba(227,201,94,0.07)" border={`${c.peel}55`} at={mac + 4} />
            <div style={{ flex: 1, position: "relative", height: 40 }}>
              <div style={{ position: "absolute", top: 19, left: 0, height: 2, width: `${arrow * 100}%`, background: `linear-gradient(90deg, ${c.peel}, ${c.bottle})` }} />
              <div style={{ position: "absolute", top: -14, width: "100%", textAlign: "center", fontFamily: mono, fontSize: 17, color: c.bottle, opacity: arrow }}>
                clonefile(2)
              </div>
            </div>
            <Box label="feat-login.img" sub="shares every block" tint="rgba(124,196,155,0.07)" border={`${c.bottle}66`} at={cue("internals", "clonefile")} />
          </div>
          <div style={{ display: "flex", gap: 14, marginTop: 34, flexWrap: "wrap" }}>
            <Chip at={cue("internals", "single", 0, -2)}>one syscall, any repo size</Chip>
            <Chip at={cue("internals", "call", 0, 2)}>mounted in ~0.7s</Chip>
          </div>
        </Column>
        <Column at={linux} os="Linux" fs="btrfs">
          <Glass tint="rgba(233,238,232,0.03)" style={{ padding: "22px 24px" }}>
            <div style={{ fontFamily: mono, fontSize: 18, color: c.dim, marginBottom: 14 }}>pool.btrfs · mounted once per repo</div>
            <Box label=".clove-golden" sub="subvolume" tint="rgba(227,201,94,0.07)" border={`${c.peel}55`} at={linux + 6} style={{ marginBottom: 12 }} />
            <div style={{ display: "flex", gap: 12 }}>
              {snaps.map((n, i) => (
                <Box key={n} label={n} sub="snapshot" tint="rgba(124,196,155,0.07)" border={`${c.bottle}66`} at={snapAt + i * 5} style={{ flex: 1 }} />
              ))}
            </div>
          </Glass>
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
    <div style={{ fontFamily: mono, fontSize, lineHeight: 1.18, background: "#0b100d", padding: 18, whiteSpace: "pre" }}>
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
        opacity: Math.min(1, p * 1.3) * (1 - back * 0.72),
        transform: `translate(-50%, -50%) translateY(${(1 - p) * 70 - back * 150}px) scale(${(0.94 + 0.06 * p) * (1 - back * 0.14)})`,
        filter: `blur(${back * 2.5}px)`,
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

/** 11. Update with Claude, prune, agents; timed to the three sentences. */
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

/** 12. Install. */
export function Outro() {
  const tag = useIn(cue("outro", "worktrees", 0, -4));
  const cmd = useIn(cue("outro", "second", 0, 8));
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
        <div style={{ display: "flex", gap: 14, marginTop: 28, opacity: cmd }}>
          {["macOS", "Linux"].map((os) => (
            <span key={os} style={{ fontFamily: mono, fontSize: 19, color: c.muted, padding: "6px 16px", borderRadius: 999, border: `1px solid ${c.rule}` }}>
              {os}
            </span>
          ))}
        </div>
        <div style={{ fontFamily: mono, fontSize: 22, color: c.dim, marginTop: 22, opacity: cmd }}>github.com/pierre-borckmans/clove</div>
      </div>
    </Stage>
  );
}
