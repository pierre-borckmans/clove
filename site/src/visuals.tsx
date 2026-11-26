// The video's visuals, as static HTML and SVG. Animation comes from CSS once
// an element is marked `.in` (see enhance.ts); without JS they show final.
import type { CSSProperties, ReactNode } from "react";

const d = (ms: number) => ({ "--d": `${ms}ms` }) as CSSProperties;

export function Mark({ size = 40 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      aria-hidden="true"
      style={{ overflow: "visible" }}
    >
      <path
        d="M73.04 29.26 A31 31 0 1 0 73.04 70.74"
        fill="none"
        stroke="var(--ink)"
        strokeWidth={19}
        strokeLinecap="round"
      />
      <g className="leaf">
        <g transform="translate(53 51) rotate(16) scale(0.33) translate(-50 -49)">
          <path
            d="M50 8 C62 28 74 52 70 76 C68 90 32 90 30 76 C26 52 38 28 50 8Z"
            fill="var(--bottle)"
          />
        </g>
      </g>
    </svg>
  );
}

export function Kicker({ children, color }: { children: ReactNode; color?: string }) {
  return (
    <p className="kicker" style={color ? { color } : undefined}>
      {children}
    </p>
  );
}

export function Term({
  title,
  children,
  className = "",
  style,
}: {
  title: string;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <figure className={`term m-0 ${className}`} style={style}>
      <div className="flex items-center gap-2 border-b border-rule px-4 py-3">
        <span className="h-2.5 w-2.5 rounded-full bg-[#e8806f]/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-[#e3c95e]/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-[#7cc49b]/70" />
        <span className="flex-1 truncate text-center font-mono text-[0.72rem] text-dim">
          {title}
        </span>
        <span className="w-12" />
      </div>
      <div className="overflow-x-auto px-5 py-4 font-mono text-[0.78rem] leading-[1.75] whitespace-pre sm:text-[0.86rem]">
        {children}
      </div>
    </figure>
  );
}

/** A terminal line that rises in after `at` ms once its section is in view. */
export function L({ at = 0, c, children }: { at?: number; c?: string; children: ReactNode }) {
  return (
    <div data-reveal style={{ ...d(at), color: c }}>
      {children}
    </div>
  );
}

export function Prompt({ cmd, args }: { cmd: string; args?: string }) {
  return (
    <>
      <span className="text-bottle">❯ </span>
      <span className="font-semibold text-ink">{cmd}</span>
      {args && <span className="text-muted"> {args}</span>}
    </>
  );
}

/* ---------------------------------------------------------------- clash */

export function Clash() {
  const agents: [string, string][] = [
    ["claude", "feat/login"],
    ["codex", "fix/race"],
    ["claude", "agent/parser"],
    ["cursor", "agent/docs"],
  ];
  const card = (i: number) => {
    const [tool, branch] = agents[i];
    return (
      <div data-reveal style={d(100 + i * 90)} className="glass glow-green px-4 py-3 font-mono">
        <div className="text-[0.75rem] text-bottle">{tool}</div>
        <div className="text-[0.95rem]">wants {branch}</div>
      </div>
    );
  };
  return (
    <div className="clash grid items-center gap-4 md:grid-cols-[1fr_1.6fr_1fr]">
      <div className="grid gap-4">
        {card(0)}
        {card(2)}
      </div>
      <div className="core glass glow-red px-6 py-6 font-mono">
        <div className="text-[0.75rem] text-dim">~/src/mono · the only checkout</div>
        <div className="mt-3 text-[1.6rem]">
          <span className="text-muted">HEAD → </span>
          <span className="text-red">fix/race</span>
        </div>
        <div className="mt-3 text-[0.82rem] leading-[1.8]">
          <L at={600} c="var(--muted)">
            codex ❯ git checkout fix/race
          </L>
          <L at={1100} c="var(--red)">
            ✗ claude's login fix, now on codex's fix/race
          </L>
        </div>
      </div>
      <div className="grid gap-4">
        {card(1)}
        {card(3)}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- worktrees */

/** git's answer: one worktree per branch, each in its own folder. */
export function Worktrees() {
  const trees: [string, string, string][] = [
    ["~/src/mono", "main", "you"],
    ["~/src/feat-login", "feat/login", "claude"],
    ["~/src/fix-race", "fix/race", "codex"],
    ["~/src/agent-parser", "agent/parser", "claude"],
  ];
  return (
    <div className="grid items-center gap-8 lg:grid-cols-[1.2fr_1fr]">
      <div className="grid gap-5">
        <div
          data-reveal
          className="glass glow-green w-fit rounded-[22px_22px_22px_6px] px-6 py-3 text-[1.6rem] italic"
        >
          "Duh. I'm not a <span className="text-bottle">caveman</span>."
        </div>
        <Term title="~/src/mono">
          <L at={200}>
            <Prompt cmd="git" args="worktree add ../feat-login -b feat/login" />
          </L>
          <L at={500} c="var(--muted)">
            Preparing worktree (new branch 'feat/login')
          </L>
          <L at={650} c="var(--muted)">
            HEAD is now at c9483ee feat(api): rate limits per team
          </L>
          <L at={900} c="var(--bottle)">
            ✓ ready in 0.9s
          </L>
        </Term>
      </div>
      <div className="grid gap-3">
        {trees.map(([dir, branch, who], i) => (
          <div
            key={dir}
            data-reveal
            style={d(300 + i * 140)}
            className={`glass flex items-center justify-between px-5 py-3.5 font-mono ${i ? "glow-green" : ""}`}
          >
            <div>
              <div className="text-[0.95rem]">{dir}</div>
              <div className="text-[0.75rem] text-muted">{branch}</div>
            </div>
            <span className="rounded-full border border-bottle/40 px-2.5 py-0.5 text-[0.72rem] text-bottle">
              {who}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- empty */

export function Empty() {
  const missing: [string, string][] = [
    ["dependencies", "node_modules · .venv · target/"],
    ["dev shell", "nix · mise · direnv"],
    ["local files", ".env · certs · local config"],
  ];
  return (
    <div className="grid items-center gap-6 lg:grid-cols-[1.5fr_1fr]">
      <Term title="~/src/feat-login" className="glow-red">
        <L>
          <Prompt cmd="make" args="dev" />
        </L>
        <L at={300} c="var(--red)">
          sh: next: command not found
        </L>
        <L at={450} c="var(--red)">
          ModuleNotFoundError: No module named 'fastapi'
        </L>
        <L at={600} c="var(--red)">
          protoc: command not found
        </L>
        <L at={1200} c="var(--bottle)">
          ✓ vibes
        </L>
      </Term>
      <div className="grid gap-3">
        {missing.map(([label, hint], i) => (
          <div key={label} data-reveal style={d(300 + i * 150)} className="glass px-5 py-4">
            <div className="font-mono text-[1.05rem]">
              <span className="text-red">✗ </span>
              <span className="line-through decoration-red/70">{label}</span>
            </div>
            <div className="mt-1 pl-6 font-mono text-[0.78rem] text-muted">{hint}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- hacks */

export function Hacks() {
  const rows: [string, string, string, string][] = [
    ["say", "This worktree has no dependencies. Setting it up first.", "", ""],
    ["run", "pnpm install                          31s", "REINSTALL", "var(--peel)"],
    ["run", "direnv allow && nix develop           41s", "REINSTALL", "var(--peel)"],
    ["err", "can't run lint here: eslint isn't installed", "GAVE UP", "var(--red)"],
    ["err", "can't regenerate the protos here: buf not on PATH", "GAVE UP", "var(--red)"],
    ["run", "cd ../mono && make proto && cp -r gen ../feat-login/", "HACK", "var(--red)"],
    ["run", "ln -s ../mono/node_modules node_modules", "HACK", "var(--red)"],
  ];
  const mark = { say: "●", run: "❯", err: "✗" } as const;
  return (
    <Term title="claude · feat/login">
      {rows.map(([kind, text, tag, col], i) => (
        <L key={text} at={i * 160}>
          <div className="flex items-center justify-between gap-6">
            <span
              style={{
                color:
                  kind === "err" ? "var(--red)" : kind === "say" ? "var(--ink)" : "var(--muted)",
              }}
            >
              {mark[kind as keyof typeof mark]} {text}
            </span>
            {tag && (
              <span
                className="rounded border px-2 text-[0.65rem] tracking-widest"
                style={{ color: col, borderColor: col }}
              >
                {tag}
              </span>
            )}
          </div>
        </L>
      ))}
    </Term>
  );
}

/* ---------------------------------------------------------------- cost */

export function Bar({
  w,
  color,
  at = 0,
  h = 8,
}: {
  w: number;
  color: string;
  at?: number;
  h?: number;
}) {
  return (
    <div
      className="fill overflow-hidden rounded-full bg-[rgba(233,238,232,0.07)]"
      style={{ height: h, ...d(at) }}
    >
      <i style={{ "--w": `${w}%`, background: color } as CSSProperties} />
    </div>
  );
}

export function Cost() {
  const rows: [string, number, number][] = [
    ["dev shell · nix + direnv", 41, 57],
    ["dependencies · pnpm, uv, cargo", 30, 42],
    ["local files · .env", 1, 2],
  ];
  return (
    <div className="grid items-center gap-10 lg:grid-cols-[1.4fr_1fr]">
      <div className="grid gap-7 font-mono text-[0.9rem]">
        {rows.map(([label, s, w], i) => (
          <div key={label} data-reveal style={d(i * 200)}>
            <div className="mb-2 flex justify-between text-muted">
              <span>{label}</span>
              <span className="text-dim">{s}s</span>
            </div>
            <Bar w={w} color="var(--peel)" at={200 + i * 400} />
          </div>
        ))}
      </div>
      <div data-reveal style={d(400)} className="grid grid-cols-3 gap-3">
        <Clock />
        <Disk />
        <Fan />
      </div>
    </div>
  );
}

function DriveIcon() {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      aria-hidden="true"
    >
      <rect x="2.5" y="13" width="19" height="7" rx="2" />
      <path d="M5.5 13 8 4.5h8l2.5 8.5" />
      <circle cx="17.5" cy="16.5" r="0.8" fill="currentColor" />
      <path d="M6 16.5h5" />
    </svg>
  );
}

function FanIcon() {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="9.5" />
      <circle cx="12" cy="12" r="2" />
    </svg>
  );
}

function WatchIcon() {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="13.5" r="8" />
      <path d="M12 13.5V9.5M9.5 2.5h5M12 2.5v3" />
    </svg>
  );
}

/** A stopwatch running past a full minute, to 72s. */
function Clock() {
  const r = 52;
  return (
    <Tile icon={<WatchIcon />} label="time">
      <svg
        className="gauge"
        width={GAUGE}
        height={GAUGE}
        viewBox="-70 -70 140 140"
        aria-hidden="true"
      >
        <circle r={r} fill="none" stroke="rgba(233,238,232,0.07)" strokeWidth={14} />
        {[0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330].map((deg) => (
          <line
            key={deg}
            y1={-r + 11}
            y2={-r + 15}
            stroke="var(--dim)"
            strokeWidth={2}
            transform={`rotate(${deg})`}
          />
        ))}
        <circle
          r={r}
          fill="none"
          stroke="var(--peel)"
          strokeWidth={14}
          pathLength={1}
          className="seg"
          transform="rotate(-90)"
          style={{ "--seg": "1 0", ...d(300) } as CSSProperties}
        />
        <circle
          r={r - 14}
          fill="none"
          stroke="var(--red)"
          strokeWidth={6}
          pathLength={1}
          className="seg"
          transform="rotate(-90)"
          style={{ "--seg": "0.2 0.8", ...d(1000) } as CSSProperties}
        />
        <g className="hand" style={{ "--rot": "432deg" } as CSSProperties}>
          <line y2={-r + 20} stroke="var(--ink)" strokeWidth={3} strokeLinecap="round" />
        </g>
        <circle r={4} fill="var(--ink)" />
        <text
          y={30}
          textAnchor="middle"
          fill="var(--ink)"
          fontFamily="var(--font-mono)"
          fontSize={17}
        >
          72s
        </text>
      </svg>
      <div className="mt-3 font-mono text-[0.8rem] text-peel">per worktree</div>
    </Tile>
  );
}

const GAUGE = 124;

function Tile({ icon, label, children }: { icon: ReactNode; label: string; children: ReactNode }) {
  return (
    <div className="glass flex flex-col items-center px-5 pt-4 pb-5">
      <div className="mb-3 flex w-full items-center gap-2 font-mono text-[0.72rem] tracking-[0.2em] text-muted uppercase">
        {icon} {label}
      </div>
      {children}
    </div>
  );
}

/** A storage ring filling up, one worktree's setup at a time. */
function Disk() {
  const used = 0.55;
  const per = 0.1;
  const r = 52;
  // Arcs as dash segments of a circle with pathLength 1, starting at 12 o'clock.
  const arc = (start: number, len: number, color: string, at: number, base = false) => (
    <circle
      key={start}
      r={r}
      fill="none"
      stroke={color}
      strokeWidth={14}
      pathLength={1}
      className={base ? undefined : "seg"}
      strokeDasharray={base ? `${len} ${1 - len}` : undefined}
      strokeDashoffset={-start}
      transform="rotate(-90)"
      style={{ "--seg": `${len - 0.008} ${1 - len + 0.008}`, ...d(at) } as CSSProperties}
    />
  );
  return (
    <Tile icon={<DriveIcon />} label="disk">
      <svg
        className="gauge"
        width={GAUGE}
        height={GAUGE}
        viewBox="-70 -70 140 140"
        aria-hidden="true"
      >
        <circle r={r} fill="none" stroke="rgba(233,238,232,0.07)" strokeWidth={14} />
        {arc(0, used, "rgba(233,238,232,0.2)", 0, true)}
        {[0, 1, 2, 3].map((i) =>
          arc(used + i * per, per, i === 3 ? "var(--red)" : "var(--peel)", 500 + i * 420),
        )}
        <text
          y={-2}
          textAnchor="middle"
          fill="var(--red)"
          fontFamily="var(--font-mono)"
          fontSize={17}
        >
          1.4 GB
        </text>
        <text
          y={17}
          textAnchor="middle"
          fill="var(--muted)"
          fontFamily="var(--font-mono)"
          fontSize={11}
        >
          free
        </text>
      </svg>
      <div className="mt-3 font-mono text-[0.8rem] text-peel">+3.1 GB each</div>
    </Tile>
  );
}

/** A laptop fan that spins up when the section comes into view. */
function Fan() {
  return (
    <Tile icon={<FanIcon />} label="fan">
      <svg
        className="fan text-red"
        width={GAUGE}
        height={GAUGE}
        viewBox="-60 -60 120 120"
        aria-hidden="true"
      >
        <circle r={56} fill="none" stroke="currentColor" strokeOpacity={0.25} strokeWidth={3} />
        <circle
          r={48}
          fill="none"
          stroke="currentColor"
          strokeOpacity={0.12}
          strokeWidth={1.5}
          strokeDasharray="2 5"
        />
        <g className="blades">
          {[0, 51, 103, 154, 206, 257, 309].map((a) => (
            <path
              key={a}
              transform={`rotate(${a})`}
              d="M0 -7 C 16 -12, 38 -30, 44 -12 C 47 -1, 24 3, 0 7 Z"
              fill="currentColor"
              fillOpacity={0.75}
            />
          ))}
        </g>
        <circle r={9} fill="var(--paper)" stroke="currentColor" strokeWidth={3} />
      </svg>
      <div className="mt-3 font-mono text-[0.8rem] text-red">6,800 rpm</div>
    </Tile>
  );
}

/* ---------------------------------------------------------------- golden */

/** Wires from one point on the left to `n` evenly spaced points on the right.
 * Stretches to its box; strokes keep their width. */
/** Wires from the left edge's middle to `ys` on the right, in a box of
 * fixed pixel size (W×H), so nothing stretches. */
/** One wire at a time: each draws in WIRE_MS, its clone card lands as it
 * arrives, then the next one starts. */
const WIRE_AT = 900;
const WIRE_MS = 600;
const WIRE_GAP = 700;
const W = 220;
const H = 256;
function Wires({ ys }: { ys: number[] }) {
  return (
    <svg
      className="draw"
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      aria-hidden="true"
      style={{ overflow: "visible" }}
    >
      <defs>
        <linearGradient id="wire" x1="0" x2="1">
          <stop offset="0" stopColor="var(--peel)" />
          <stop offset="1" stopColor="var(--bottle)" />
        </linearGradient>
      </defs>
      {ys.map((y, i) => {
        const y0 = H / 2 + (i - (ys.length - 1) / 2) * 7;
        const p = `M 0 ${y0} C ${W / 2} ${y0}, ${W / 2} ${y}, ${W} ${y}`;
        return (
          <g key={p}>
            <path
              className="wire"
              d={p}
              pathLength={1}
              fill="none"
              stroke="url(#wire)"
              strokeWidth={12}
              strokeOpacity={0.16}
              strokeLinecap="round"
              style={{ ...d(WIRE_AT + i * WIRE_GAP), "--dur": `${WIRE_MS}ms` } as CSSProperties}
            />
            <path
              className="wire"
              d={p}
              pathLength={1}
              fill="none"
              stroke="url(#wire)"
              strokeWidth={4}
              strokeLinecap="round"
              style={{ ...d(WIRE_AT + i * WIRE_GAP), "--dur": `${WIRE_MS}ms` } as CSSProperties}
            />
            <path
              className="spark"
              d={p}
              pathLength={1}
              fill="none"
              stroke="var(--ink)"
              strokeWidth={8}
              strokeLinecap="round"
              strokeDasharray="0.001 0.999"
              style={
                {
                  // Its spark sets off just behind the drawing wire.
                  "--spark": `${(WIRE_AT + i * WIRE_GAP + 120) / 1000}s`,
                } as CSSProperties
              }
            />
          </g>
        );
      })}
    </svg>
  );
}

export function Golden() {
  const clones = ["feat/login", "agent/parser", "fix/race", "pr/4053"];
  return (
    <div className="grid items-center gap-y-8 lg:grid-cols-[1fr_220px_0.9fr]">
      <div
        data-reveal
        className="glass relative z-10 px-7 py-7"
        style={{
          borderColor: "rgba(227,201,94,0.35)",
          background: "rgba(227,201,94,0.06)",
          boxShadow: "0 0 70px rgba(227,201,94,0.12)",
        }}
      >
        <div className="flex items-center gap-3 text-[1.9rem]">
          <Mark size={34} /> golden image
        </div>
        <div className="mt-3 font-mono text-[0.85rem] leading-[1.9]">
          <div className="text-muted">main @ c9483ee</div>
          <div data-reveal style={d(250)}>
            <span className="text-bottle">✓</span> dependencies · pnpm, uv, cargo
          </div>
          <div data-reveal style={d(420)}>
            <span className="text-bottle">✓</span> dev shell · nix, mise, direnv
          </div>
          <div data-reveal style={d(590)}>
            <span className="text-bottle">✓</span> local files · .env
          </div>
        </div>
      </div>
      {/* Four cards of equal height, 1rem apart: the wires end on their centres. */}
      <div className="hidden lg:block">
        <Wires ys={[26, 94, 162, 230]} />
      </div>
      <div className="relative grid gap-4">
        <p data-reveal style={d(WIRE_AT)} className="kicker absolute -top-9 left-0 !text-[0.68rem]">
          copy-on-write clones
        </p>
        {clones.map((cl, i) => (
          <div
            key={cl}
            data-reveal
            style={d(WIRE_AT + i * WIRE_GAP + WIRE_MS - 250)}
            className="glass flex h-[3.25rem] items-center justify-between px-5 font-mono text-[0.95rem]"
          >
            <span>{cl}</span>
            <span className="text-bottle">+6 MB</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- detect */

export function Detect() {
  const tools: [string, string][] = [
    ["mise", "tool versions"],
    ["direnv", "environment"],
    ["nix", "dev shell"],
    ["pnpm", "node"],
    ["bun", "node"],
    ["yarn", "node"],
    ["npm", "node"],
    ["uv", "python"],
    ["cargo", "rust"],
    [".env", "local files"],
  ];
  return (
    <div className="grid items-center gap-8 lg:grid-cols-[1.4fr_1fr]">
      <div className="chips grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-5">
        {tools.map(([n, kind], i) => (
          <div key={n} className="chip-lit glass px-4 py-3 font-mono" style={d(i * 110)}>
            <div className="text-[1.05rem]">
              <span className="text-bottle">✓</span> {n}
            </div>
            <div className="text-[0.7rem] text-muted">{kind}</div>
          </div>
        ))}
      </div>
      <Term title=".clove.toml · shared with the team">
        <L at={1200} c="var(--muted)">
          [[add_step]]
        </L>
        <L at={1300}>name = "codegen"</L>
        <L at={1400}>warm = "make proto"</L>
        <L at={1500}>setup = "make proto"</L>
        <L at={1600} c="var(--peel)">
          watch = ["proto/"]
        </L>
      </Term>
    </div>
  );
}

/* ---------------------------------------------------------------- fresh */

export function Fresh() {
  const steps: [string, string, boolean][] = [
    ["pnpm", "unchanged · reused", false],
    ["cargo", "unchanged · reused", false],
    ["codegen", "proto/ changed · rerun", true],
  ];
  return (
    <div className="grid items-center gap-6 lg:grid-cols-[0.9fr_1.2fr_0.9fr]">
      <Term title="~/src/mono">
        <L>
          <Prompt cmd="git" args="pull" />
        </L>
        <L at={400} c="var(--muted)">
          Fast-forward · 12 commits
        </L>
      </Term>
      <div
        data-reveal
        style={{
          ...d(500),
          borderColor: "rgba(227,201,94,0.35)",
          background: "rgba(227,201,94,0.06)",
        }}
        className="glass px-6 py-6"
      >
        <div className="mb-3 flex items-baseline justify-between">
          <span className="text-[1.6rem]">golden image</span>
          <span className="font-mono text-[0.8rem] text-bottle">gen 15 ✓</span>
        </div>
        {steps.map(([n, what, ran], i) => (
          <L key={n} at={900 + i * 300} c={ran ? "var(--peel)" : "var(--muted)"}>
            <div className="flex gap-4 font-mono text-[0.85rem] leading-[2]">
              <span className="w-4">{ran ? "↻" : "✓"}</span>
              <span className="w-20 text-ink">{n}</span>
              <span>{what}</span>
            </div>
          </L>
        ))}
        <div className="mt-4">
          <Bar w={100} color="linear-gradient(90deg, var(--peel), var(--bottle))" at={800} h={6} />
        </div>
      </div>
      <div className="grid gap-3">
        {["feat/login", "agent/parser", "fix/race"].map((n, i) => (
          <div
            key={n}
            data-reveal
            style={d(300 + i * 120)}
            className="glass glow-green flex items-center justify-between px-5 py-3.5 font-mono text-[0.9rem]"
          >
            <span>{n}</span>
            <span className="pulse text-[0.75rem] text-bottle">● working</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- internals */

function Pool({
  os,
  fs,
  caption,
  golden,
  goldenSub,
  link,
  cloneSub,
  chips,
}: {
  os: string;
  fs: string;
  caption: string;
  golden: string;
  goldenSub: string;
  link: string;
  cloneSub: string;
  chips: string[];
}) {
  return (
    <div data-reveal className="min-w-0">
      <div className="mb-4 flex items-baseline gap-3">
        <span className="text-[2.4rem] tracking-[-0.02em]">{os}</span>
        <span className="font-mono text-[0.8rem] tracking-widest text-bottle">{fs}</span>
      </div>
      <div className="glass px-5 py-5">
        <div className="mb-3 font-mono text-[0.75rem] text-dim">{caption}</div>
        <div
          className="glass px-4 py-3 font-mono"
          style={{ borderColor: "rgba(227,201,94,0.35)", background: "rgba(227,201,94,0.06)" }}
        >
          <div className="text-[1rem]">{golden}</div>
          <div className="text-[0.72rem] text-muted">{goldenSub}</div>
        </div>
        <div className="my-3 flex items-center gap-3">
          <div className="fill h-[2px] flex-1">
            <i
              style={
                {
                  "--w": "100%",
                  background: "linear-gradient(90deg, var(--peel), var(--bottle))",
                } as CSSProperties
              }
            />
          </div>
          <span className="font-mono text-[0.72rem] text-bottle">{link}</span>
        </div>
        <div className="grid grid-cols-3 gap-2">
          {["feat-login", "fix-race", "agent/parser"].map((n, i) => (
            <div
              key={n}
              data-reveal
              style={d(600 + i * 120)}
              className="glass px-3 py-2.5 font-mono"
            >
              <div className="truncate text-[0.85rem]">{n}</div>
              <div className="text-[0.68rem] text-muted">{cloneSub}</div>
            </div>
          ))}
        </div>
      </div>
      <div className="mt-4 flex flex-wrap gap-2">
        {chips.map((c) => (
          <span
            key={c}
            className="rounded-full border border-bottle/40 bg-bottle/10 px-3 py-1 font-mono text-[0.75rem] text-bottle"
          >
            {c}
          </span>
        ))}
      </div>
    </div>
  );
}

export function Internals() {
  return (
    <div className="grid gap-10 md:grid-cols-2">
      <Pool
        os="macOS"
        fs="APFS"
        caption="disk images · one per clone"
        golden="golden.img"
        goldenSub="the warmed image"
        link="clonefile(2)"
        cloneSub="image clone"
        chips={["one syscall, any repo size", "mounted in ~0.7s"]}
      />
      <Pool
        os="Linux"
        fs="btrfs"
        caption="pool.btrfs · mounted once per repo"
        golden=".clove-golden"
        goldenSub="subvolume"
        link="subvolume snapshot"
        cloneSub="snapshot"
        chips={["~8 ms per clone", "no mount per clone"]}
      />
    </div>
  );
}

/* ---------------------------------------------------------------- numbers */

export function Race() {
  return (
    <div className="grid items-center gap-6 lg:grid-cols-[1.1fr_1fr]">
      <Term title="~/src/mono">
        <L>
          <Prompt cmd="clove" args="new feat/login" />
        </L>
        <L at={300}>{"  ✓ clone image    0.00s"}</L>
        <L at={400}>{"  ✓ mount          0.70s"}</L>
        <L at={500}>{"  ✓ checkout       0.21s"}</L>
        <L at={600} c="var(--dim)">
          {"  · setup direnv   unchanged"}
        </L>
        <L at={700} c="var(--dim)">
          {"  · setup pnpm     unchanged"}
        </L>
        <L at={1000} c="var(--bottle)">
          <span className="font-semibold">● feat-login ready in 1.0s</span>
        </L>
      </Term>
      <div data-reveal style={d(200)} className="glass px-7 py-7 font-mono">
        <div className="mb-2 flex justify-between text-[0.85rem] text-muted">
          <span>git worktree + setup</span>
          <span className="text-peel">72s · 3.1 GB</span>
        </div>
        <Bar w={100} color="var(--peel)" at={300} h={12} />
        <div className="mt-6 mb-2 flex justify-between text-[0.85rem] text-muted">
          <span>clove new</span>
          <span className="text-bottle">1.0s · 6 MB</span>
        </div>
        <Bar w={1.4} color="var(--bottle)" at={300} h={12} />
        <div className="mt-6 font-serif text-[1.9rem]">
          <i className="text-bottle">70×</i> faster, <i className="text-bottle">500×</i> less disk
        </div>
        <div className="mt-2 flex gap-6 text-[0.82rem] text-muted">
          <span>fan · idle</span>
          <span>disk · +6 MB</span>
        </div>
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- shots */

export function Window({
  title,
  src,
  alt,
  className = "",
}: {
  title: string;
  src: string;
  alt: string;
  className?: string;
}) {
  return (
    <figure data-reveal className={`term m-0 ${className}`}>
      <div className="flex items-center gap-2 border-b border-rule px-4 py-3">
        <span className="h-2.5 w-2.5 rounded-full bg-[#e8806f]/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-[#e3c95e]/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-[#7cc49b]/70" />
        <span className="flex-1 text-center font-mono text-[0.72rem] text-dim">{title}</span>
        <span className="w-12" />
      </div>
      <img src={src} alt={alt} loading="lazy" className="block w-full" />
    </figure>
  );
}
