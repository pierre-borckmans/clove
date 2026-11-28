// Scenes and shots drawn with the app's map (garlic.tsx): the desktop app in
// Surfaces, merged clones pulled off in Features, and checkpoints and forks.
import { interpolate, useCurrentFrame } from "remotion";
import {
  AppFrame,
  Badge,
  baseOf,
  Bulb,
  Clove,
  Cursor,
  ease,
  easeOut,
  g,
  Label,
  place,
  ramp,
  Ring,
  SKIN,
  type Spot,
  Stem,
  Toasts,
} from "./garlic";
import { c, cue, Kicker, mono, serif, Stage, useSceneLength, Words } from "./ui";

const spin = (frame: number, k = 0) => "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"[Math.floor(frame / 3 + k) % 10];

/* ------------------------------------------------------- the desktop app */

const RING: Spot[] = [
  { name: "feat-onboarding-checklist", deg: -6, state: "modified", parts: [["modified", g.amber], ["↑1"]] },
  { name: "fix-cache-stampede", deg: -36, state: "clean", parts: [["↑1"], ["#1441"]] },
  { name: "review-pr-1432", deg: -72, state: "clean", parts: [["clean"]] },
  { name: "exp-streaming-ssr", deg: -112, state: "unmounted", age: 3, parts: [["unmounted"]] },
  { name: "release-2.14", deg: -150, state: "attention", age: 2, parts: [["cargo failed", g.amber]] },
  { name: "chore-bump-vite", deg: 172, state: "merged", age: 1, parts: [["merged", c.bottle], ["↑2 ↓11"]] },
  { name: "hotfix-cache-headers", deg: 128, state: "modified", age: 1, parts: [["modified", g.amber], ["↑1 ↓9"]] },
  { name: "feat-billing-v2", deg: 58, state: "merged", skins: 2, parts: [["merged", c.bottle], ["modified", g.amber], ["↑4 ↓2"]] },
  { name: "spike-search-indexer", deg: 18, state: "clean", parts: [["↑6"]] },
];

function Row({ k, v, color }: { k: string; v: string; color?: string }) {
  return (
    <div style={{ display: "flex", gap: 16, fontSize: 14, lineHeight: 1.9 }}>
      <span style={{ width: 92, color: g.muted }}>{k}</span>
      <span style={{ color: color ?? g.text, fontFamily: mono, fontSize: 13 }}>{v}</span>
    </div>
  );
}

function Panel() {
  const bars: [string, number, string][] = [
    ["clone image", 0.03, "19ms"],
    ["mount", 0.08, "301ms"],
    ["setup pnpm", 0.85, "6.82s"],
  ];
  return (
    <div style={{ fontFamily: serif, color: g.text }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 10 }}>
        <span style={{ fontSize: 18, fontWeight: 500 }}>feat-billing-v2</span>
        <span style={{ fontSize: 13, color: g.muted }}>#1402 · merged · gen 14</span>
        <span style={{ marginLeft: "auto", fontSize: 13, background: "#2f6b4c", color: "#fff", borderRadius: 7, padding: "4px 10px" }}>VS Code</span>
      </div>
      <div style={{ display: "flex", gap: 40, marginTop: 12 }}>
        <div>
          <div style={{ fontSize: 11, letterSpacing: 1.5, color: g.muted, marginBottom: 4 }}>BRANCH</div>
          <Row k="Branch" v="feat/billing-v2" />
          <Row k="Changes" v="7 files" color={g.amber} />
          <Row k="Ahead" v="↑4 ↓2" />
        </div>
        <div>
          <div style={{ fontSize: 11, letterSpacing: 1.5, color: g.muted, marginBottom: 4 }}>CHECKPOINTS & LINEAGE</div>
          <Row k="Checkpoints" v="2 · latest 4m ago" />
          <Row k="Forked from" v="the image" />
          <Row k="Forks" v="0" />
        </div>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 11, letterSpacing: 1.5, color: g.muted, marginBottom: 4 }}>SETUP & DISK</div>
          {bars.map(([l, v, t]) => (
            <div key={l} style={{ display: "flex", alignItems: "center", gap: 10, fontFamily: mono, fontSize: 12, lineHeight: 1.9 }}>
              <span style={{ width: 110, color: g.muted, whiteSpace: "nowrap" }}>{l}</span>
              <span style={{ flex: 1, height: 4, borderRadius: 4, background: g.line }}>
                <span style={{ display: "block", width: `${v * 100}%`, height: "100%", borderRadius: 4, background: c.bottle }} />
              </span>
              <span style={{ width: 46, textAlign: "right" }}>{t}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/** The desktop app on the map: the bulb, cloves sprouting round it, one picked and its panel sliding up. */
export function AppMap({ at }: { at: number }) {
  const frame = useCurrentFrame();
  const W = 1240;
  const H = 760;
  const bx = 505;
  const by = 300;
  const pick = at + 74;
  const panel = easeOut(ramp(frame, pick + 4, pick + 20));
  const lift = panel * 150;
  const spots = RING.map((s, i) => ({ ...s, at: at + 12 + i * 4 }));
  const target = spots.find((s) => s.name === "feat-billing-v2")!;
  const p = place(target, bx, by, 70, 230, 34);
  // The cursor glides to the clove, clicks it, and stays.
  const m = easeOut(ramp(frame, pick - 22, pick - 2));
  const cx = interpolate(m, [0, 1], [880, p.x + 4]);
  const cy = interpolate(m, [0, 1], [560, p.y + 4]) - lift;
  const down = frame >= pick && frame < pick + 5;
  return (
    <AppFrame
      width={W}
      height={H}
      panel={panel}
      panelContent={<Panel />}
      overlay={<Toasts items={[{ at: at + 40, until: pick + 30, title: "Refreshing the image", sub: "mono · setup pnpm", progress: [at + 40, at + 100] }]} style={{ right: 18, top: 16 }} scale={0.9} />}
    >
      <g transform={`translate(0 ${-lift})`}>
        <Bulb x={bx} y={by} r={70} appear={easeOut(ramp(frame, at + 2, at + 16))} name="mono" sub="gen 14 · main" labelSize={15} breath={frame > at + 40 && frame < at + 100 ? 0.5 - 0.5 * Math.cos((frame - at - 40) / 7) : 0} />
        <Ring spots={spots} bx={bx} by={by} r={70} ring={230} length={34} labelSize={15} selected={frame >= pick ? "feat-billing-v2" : undefined} />
      </g>
      {frame >= pick - 22 && <Cursor x={cx} y={cy} down={down} />}
    </AppFrame>
  );
}

/* -------------------------------------------- merged clones pulled off */

/** Merged clones pulled off the map one after the other: the stem strains and reddens, the clove flies off. */
export function PruneMap({ at }: { at: number }) {
  const frame = useCurrentFrame();
  const bx = 330;
  const by = 270;
  const r = 62;
  const ring = 190;
  const len = 32;
  const spots: Spot[] = [
    { name: "feat-billing-v2", deg: 30, state: "merged", parts: [["merged", c.bottle], ["#1402"]] },
    { name: "review-pr-1432", deg: -60, state: "clean", parts: [["clean"]] },
    { name: "fix-cache-stampede", deg: -10, state: "clean", parts: [["↑1"], ["#1441"]] },
    { name: "hotfix-cache-headers", deg: 145, state: "modified", age: 1, parts: [["modified", g.amber]] },
    { name: "chore-bump-vite", deg: 195, state: "merged", age: 1, parts: [["merged", c.bottle], ["#1398"]] },
    { name: "release-2.14", deg: -130, state: "attention", age: 2, parts: [["cargo failed", g.amber]] },
  ];
  const pulled: Record<string, number> = { "feat-billing-v2": at + 10, "chore-bump-vite": at + 24 };
  const still = spots.filter((s) => !(s.name in pulled));
  return (
    <div style={{ position: "relative", width: 820, height: 540, borderRadius: 16, overflow: "hidden", border: `1px solid ${c.rule}`, boxShadow: "0 60px 160px rgba(0,0,0,0.65)", background: g.bg, backgroundImage: "radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)", backgroundSize: "22px 22px" }}>
      <svg width={820} height={540} style={{ position: "absolute", inset: 0 }}>
        <Bulb x={bx} y={by} r={r} name="mono" sub="gen 14 · main" labelSize={14} />
        <Ring spots={still} bx={bx} by={by} r={r} ring={ring} length={len} labelSize={14} />
        {spots
          .filter((s) => s.name in pulled)
          .map((s) => {
            const t0 = pulled[s.name];
            const p = place(s, bx, by, r, ring, len);
            const strain = ramp(frame, t0, t0 + 16);
            const fly = ramp(frame, t0 + 18, t0 + 34);
            const recoil = ramp(frame, t0 + 18, t0 + 28);
            // Pulled outward a little as the stem strains, then off.
            const pullD = 18 * ease(strain) + 420 * fly * fly;
            const x = p.x + Math.cos(p.th) * pullD;
            const y = p.y + Math.sin(p.th) * pullD;
            const base = baseOf(x, y, p.angle, len);
            const cos = Math.cos(p.th);
            return (
              <g key={s.name}>
                {recoil < 1 && (
                  <Stem
                    x0={p.x0}
                    y0={p.y0}
                    x1={fly > 0 ? interpolate(recoil, [0, 1], [p.x1, p.x0]) : base.x}
                    y1={fly > 0 ? interpolate(recoil, [0, 1], [p.y1, p.y0]) : base.y}
                    bend={0.1}
                    strain={strain}
                    opacity={1 - recoil}
                  />
                )}
                <Clove x={x} y={y} angle={p.angle + fly * 70} length={len} state={s.state} age={s.age} opacity={1 - fly} />
                <Label x={p.x + cos * len * 0.75} y={p.y + Math.sin(p.th) * len * 0.75 - 7} name={s.name} parts={s.parts} align={cos > 0.35 ? "start" : cos < -0.35 ? "end" : "middle"} size={14} opacity={1 - ramp(frame, t0 + 14, t0 + 22)} />
              </g>
            );
          })}
      </svg>
      <Toasts
        items={[
          { at: at + 30, title: "Pruned feat-billing-v2", sub: "PR #1402 merged · branch kept", action: "Undo" },
          { at: at + 44, title: "Pruned chore-bump-vite", sub: "PR #1398 merged · branch kept", action: "Undo" },
        ]}
        style={{ right: 16, bottom: 16 }}
        scale={0.82}
      />
    </div>
  );
}

/* ------------------------------------------------- checkpoints and forks */

/** A small pill by a clove: which agent is in it, and what it's doing. */
function AgentChip({ x, y, text, color = c.bottle, opacity = 1 }: { x: number; y: number; text: string; color?: string; opacity?: number }) {
  return (
    <foreignObject x={x} y={y} width={360} height={44} opacity={opacity} style={{ overflow: "visible" }}>
      <div style={{ display: "inline-flex", alignItems: "center", gap: 10, padding: "6px 14px", borderRadius: 999, background: "rgba(20,26,23,0.92)", border: `1.5px solid ${color}66`, fontFamily: mono, fontSize: 17, color, whiteSpace: "nowrap" }}>{text}</div>
    </foreignObject>
  );
}

/** One beat of the headline: in at `at`, out at `until`. */
function Beat({ at, until, kicker, text, accent }: { at: number; until?: number; kicker: string; text: string; accent: string[] }) {
  const frame = useCurrentFrame();
  if (frame < at - 1 || (until !== undefined && frame > until + 8)) return null;
  const out = until === undefined ? 0 : ramp(frame, until, until + 8);
  return (
    <div style={{ position: "absolute", left: 170, top: 170, width: 700, opacity: 1 - out, transform: `translateY(${-out * 20}px)` }}>
      <Kicker delay={at}>{kicker}</Kicker>
      <div style={{ height: 22 }} />
      <Words text={text} size={64} delay={at + 3} accent={accent} />
    </div>
  );
}

/** Checkpoint a clone, wreck it, rewind it; then fork it three times, and fold the forks. */
export function Forks() {
  const frame = useCurrentFrame();
  const len = useSceneLength();
  const ck = cue("forks", "checkpoint", 0, 4);
  const agentAt = cue("forks", "agent", 0, -2);
  const wreck = cue("forks", "wrecks", 0, -2);
  const rw = cue("forks", "rewind", 0, -2);
  const fk = cue("forks", "fork", 0, -2);
  const three = cue("forks", "three", 0, -6);
  const fold = Math.min(cue("forks", "try", 0, 16), len - 60);

  // The bulb, low left; the clove up and to its right; forks further out.
  const bx = 690;
  const by = 760;
  const br = 112;
  const L = 104;
  const main: Spot = { name: "feat-login", deg: -38, state: "clean", ring: 430, length: L };
  const p = place(main, bx, by, br, 430, L);

  // Wreck: amber while the agent works, rose when it breaks things, cream again on rewind.
  const working = ramp(frame, agentAt + 6, agentAt + 16);
  const broken = ramp(frame, wreck, wreck + 8);
  const restored = frame >= rw + 3;
  const tint = restored
    ? SKIN.clean
    : broken > 0
      ? { fill: `color-mix(in oklab, ${SKIN.attention.fill} ${broken * 100}%, ${SKIN.modified.fill})`, stroke: `color-mix(in oklab, ${SKIN.attention.stroke} ${broken * 100}%, ${SKIN.modified.stroke})` }
      : working > 0
        ? { fill: `color-mix(in oklab, ${SKIN.modified.fill} ${working * 100}%, ${SKIN.clean.fill})`, stroke: `color-mix(in oklab, ${SKIN.modified.stroke} ${working * 100}%, ${SKIN.clean.stroke})` }
        : SKIN.clean;
  const shake = frame >= wreck && frame < rw ? Math.sin(frame * 1.9) * 5 * Math.max(0, 1 - (frame - wreck) / 14) : 0;
  const skins = frame < ck ? 0 : Math.min(1, (frame - ck) / 12);
  const peel = ramp(frame, rw, rw + 22);
  const glow = frame >= rw ? Math.max(0, 1 - Math.abs(frame - rw - 10) / 16) : 0;

  // Fork: the cursor grabs the clove with Option held, a copy tears free and follows it out.
  const grab = fk - 4;
  const release = fk + 16;
  const forks = [
    { name: "feat-login-a", deg: -64, d: 270, at: release, agent: "claude" },
    { name: "feat-login-b", deg: -30, d: 300, at: three, agent: "codex" },
    { name: "feat-login-c", deg: 2, d: 280, at: three + 6, agent: "cursor" },
  ];
  const tip = { x: p.x + Math.sin((p.angle * Math.PI) / 180) * L * 0.42, y: p.y - Math.cos((p.angle * Math.PI) / 180) * L * 0.42 };
  const FL = 72;
  const folded = ease(ramp(frame, fold, fold + 18));
  const pos = (f: (typeof forks)[number]) => {
    const th = (f.deg * Math.PI) / 180;
    return { x: tip.x + Math.cos(th) * f.d, y: tip.y + Math.sin(th) * f.d, angle: f.deg + 90 };
  };
  // Where folded forks huddle: just behind the parent.
  const huddle = (i: number) => {
    // Fanned behind the parent, tips peeking out past its shoulders.
    const a = ((p.angle - 28 + i * 28) * Math.PI) / 180;
    return { x: p.x + Math.sin(a) * L * 0.22, y: p.y - Math.cos(a) * L * 0.22, angle: p.angle - 28 + i * 28 };
  };

  const f0 = pos(forks[0]);
  const drag = easeOut(ramp(frame, fk, release));
  const ghost = { x: interpolate(drag, [0, 1], [p.x, f0.x]), y: interpolate(drag, [0, 1], [p.y, f0.y]) };
  const cur =
    frame < grab
      ? { x: interpolate(easeOut(ramp(frame, grab - 18, grab)), [0, 1], [1500, p.x + 10]), y: interpolate(easeOut(ramp(frame, grab - 18, grab)), [0, 1], [820, p.y + 10]) }
      : frame < release
        ? { x: ghost.x + 10, y: ghost.y + 10 }
        : { x: f0.x + 10 + (frame - release) * 3, y: f0.y + 10 + (frame - release) * 2 };
  const curOpacity = ramp(frame, grab - 18, grab - 10) * (1 - ramp(frame, release + 6, release + 16));

  const state: [string, string?][] = restored
    ? frame < rw + 26
      ? [["rewound to checkpoint 1", c.bottle]]
      : [["clean"], ["1 checkpoint"]]
    : broken > 0
      ? [["tests failing", g.rose], ["214 files", g.muted]]
      : working > 0
        ? [["modified", g.amber], ["↑3"]]
        : skins > 0
          ? [["clean"], ["1 checkpoint", c.bottle]]
          : [["clean"]];

  return (
    <Stage push={0.025}>
      <Beat at={2} until={rw - 6} kicker="clove checkpoint" text="Checkpoint before the agent starts." accent={["Checkpoint"]} />
      <Beat at={rw - 4} until={fk - 8} kicker="clove rewind" text="Wrecked? Rewind in a second." accent={["Rewind"]} />
      <Beat at={fk - 6} kicker="clove fork" text="Or fork it. Three agents, three tries." accent={["fork"]} />
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0, overflow: "visible" }}>
        <Bulb x={bx} y={by} r={br} name="mono" sub="gen 14 · main" labelSize={20} appear={easeOut(ramp(frame, 0, 14))} deepest={2} />
        <Ring
          spots={[
            { name: "fix-race", deg: -100, state: "modified", parts: [["modified", g.amber]], at: 2 },
            { name: "agent-parser", deg: 190, state: "clean", age: 1, parts: [["↑2"]], at: 6 },
            { name: "pr-4053", deg: 20, state: "merged", parts: [["merged", c.bottle]], at: 10 },
          ]}
          bx={bx}
          by={by}
          r={br}
          ring={250}
          length={54}
          labelSize={18}
          dim={() => 0.4}
        />
        {/* The clove, on its stem from the rim. */}
        <Stem x0={p.x0} y0={p.y0} x1={p.x1} y1={p.y1} bend={0.06} draw={ease(ramp(frame, 0, 16))} width={2.4} />
        {/* Fork stems, clove to clove; they pull back in as the forks fold. */}
        {forks.map((f, i) => {
          const t = i === 0 ? (frame >= release ? 1 : 0) : ease(ramp(frame, f.at, f.at + 12));
          if (t <= 0) return null;
          const q = pos(f);
          const b = baseOf(q.x, q.y, q.angle, FL);
          return <Stem key={f.name} x0={tip.x} y0={tip.y} x1={interpolate(folded, [0, 1], [b.x, tip.x])} y1={interpolate(folded, [0, 1], [b.y, tip.y])} bend={0.12} draw={t} width={2} opacity={1 - folded} />;
        })}
        {/* While dragging: the copy's stem, dashed and straining. */}
        {frame >= fk && frame < release && (
          <Stem x0={tip.x} y0={tip.y} x1={baseOf(ghost.x, ghost.y, forks[0].deg + 90, FL).x} y1={baseOf(ghost.x, ghost.y, forks[0].deg + 90, FL).y} bend={0.12} old width={2} lit />
        )}
        {forks.map((f, i) => {
          const appear = i === 0 ? (frame >= release ? 1 : 0) : easeOut(ramp(frame, f.at + 6, f.at + 20));
          if (appear <= 0) return null;
          const q = pos(f);
          const h = huddle(i);
          const x = interpolate(folded, [0, 1], [q.x, h.x]);
          const y = interpolate(folded, [0, 1], [q.y, h.y]);
          const pop = i === 0 ? 1 + 0.12 * Math.max(0, 1 - (frame - release) / 6) : 1;
          return (
            <g key={f.name}>
              <Clove x={x} y={y} angle={interpolate(folded, [0, 1], [q.angle, h.angle])} length={interpolate(folded, [0, 1], [FL, L * 0.6])} grow={appear * pop} opacity={1 - folded * 0.5} state="modified" />
              <Label x={q.x + 52} y={q.y - 14} name={f.name} parts={[[f.agent, c.bottle], ["working"]]} size={20} opacity={appear * (1 - folded)} />
            </g>
          );
        })}
        <g transform={`translate(${shake} 0)`}>
          {/* The wrecked flesh peels off on rewind, the clove beneath as it was. */}
          {peel > 0 && peel < 1 && (
            <Clove x={p.x + Math.cos(p.th) * 70 * peel} y={p.y + Math.sin(p.th) * 70 * peel} angle={p.angle + 30 * peel} length={L * (1 + 0.25 * peel)} state="attention" opacity={1 - peel} />
          )}
          <Clove x={p.x} y={p.y} angle={p.angle} length={L} tint={tint} skins={skins} skinGlow={glow} grow={easeOut(ramp(frame, 6, 22))} selected={frame >= fk - 4 && frame < release} />
        </g>
        {/* The copy being pulled: translucent, dashed. */}
        {frame >= fk && frame < release && (
          <g opacity={0.75}>
            <Clove x={ghost.x} y={ghost.y} angle={interpolate(drag, [0, 1], [p.angle, forks[0].deg + 90])} length={FL} state="unmounted" tint={{ fill: "color-mix(in oklab, #ddd3a6 55%, transparent)", stroke: "#e3c95e" }} />
          </g>
        )}
        {frame >= fk && frame < release && (
          <text x={ghost.x + 30} y={ghost.y - 46} fontFamily={serif} fontSize={20} fill={c.bottle}>
            let go to fork
          </text>
        )}
        <Label x={p.x + 70} y={p.y + 30} name="feat-login" parts={state} size={24} opacity={ramp(frame, 10, 20)} />
        {frame >= agentAt && frame < rw && (
          <AgentChip x={p.x - 300} y={p.y - 120} text={broken > 0 ? "claude ✗ rm -rf src/auth" : `claude ${spin(frame)} editing 214 files`} color={broken > 0 ? g.rose : c.bottle} opacity={ramp(frame, agentAt, agentAt + 8) * (1 - ramp(frame, rw - 6, rw))} />
        )}
        <Badge x={p.x + 8} y={p.y + L * 0.62} n={3} size={22} appear={easeOut(ramp(frame, fold + 14, fold + 26))} />
        {curOpacity > 0 && (
          <g>
            <Cursor x={cur.x} y={cur.y} opacity={curOpacity} down={frame >= grab && frame < release} />
            <foreignObject x={cur.x + 26} y={cur.y + 22} width={60} height={40} opacity={curOpacity * ramp(frame, grab - 6, grab)}>
              <div style={{ fontFamily: mono, fontSize: 18, color: g.text, background: "rgba(40,46,42,0.95)", border: `1px solid ${g.line}`, borderRadius: 7, padding: "2px 8px", display: "inline-block" }}>⌥</div>
            </foreignObject>
          </g>
        )}
      </svg>
      <Toasts
        items={[
          { at: ck + 6, until: rw, title: "Checkpoint 1 saved", sub: "feat-login · 0.2s" },
          { at: wreck + 4, until: rw + 30, title: "claude: tests failing", sub: "214 files changed", tone: g.rose },
          { at: rw, until: fk + 10, title: "Rewound feat-login", sub: "to checkpoint 1 · 0.9s", progress: [rw, rw + 27] },
          { at: three + 14, title: "Forked feat-login ×3", sub: "feat-login-a · -b · -c" },
        ]}
        style={{ right: 110, bottom: 100 }}
        scale={1.25}
      />
    </Stage>
  );
}
