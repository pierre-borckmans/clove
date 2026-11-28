// biome-ignore-all lint/a11y/noSvgWithoutTitle: no tooltips on the map; its parts carry their own accessible names.
// biome-ignore-all lint/a11y/useSemanticElements: SVG has no <button> or <fieldset>; these groups take the roles.
// biome-ignore-all lint/a11y/noInteractiveElementToNoninteractiveRole: an SVG <g> is not interactive; biome reads it as one.
// The map as one SVG scene on one clock. React renders what is on the map and
// how it looks (classes, text); every position, size and fade is written by
// the scene's frame loop (scene.ts) straight into attributes, so stems,
// cloves, labels and badges always move together.

import { type CSSProperties, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { CLOVE_PATH } from "@/components/glyph";
import { MOD } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { BULB_R, BulbArt, type BulbShape, bulbShape, contourPath, contourRadius, rimPath } from "./bulb";
import {
  BULB_GEN_Y,
  BULB_NAME_Y,
  cloneState,
  cloveLength,
  LABEL_NAME_Y,
  LABEL_STATE_Y,
  MAX_AGE,
  SANS,
  type StatePart,
  textWidth,
} from "./geometry";
import { CORE_RADIUS, edgeGeom, quadPath, REPO_LABEL_Y, type RingNode, type RingsLayout, stemPath } from "./layout";
import { forkPullOn, isDimmed, type MapCtx, menuTrigger, pullOn, skinClasses } from "./nodes";
import { FAIL_MS, FLY_MS, LABEL_IN_MS, RECOIL_MS, type Scene, SHIFT_MS, SWAP_MS, SWEEP_MS } from "./scene";

/** A layout node, with its label and leader as offsets from the clove. */
export type SceneNode = RingNode & {
  rel: { x: number; y: number };
  leaderRel: readonly [number, number, number, number] | null;
};
export type MapScene = Scene<SceneNode>;

/** Element handles for one clove, filled by ref callbacks, written by the frame loop. */
type Parts = {
  g?: SVGGElement | null;
  rot?: SVGGElement | null;
  huddle?: SVGGElement | null;
  skins?: SVGGElement | null;
  label?: SVGGElement | null;
  edge?: SVGPathElement | null;
  leader?: SVGGElement | null;
  badge?: SVGGElement | null;
  root?: SVGGElement | null;
};

const SYMBOL = 17;
const deg = (rad: number) => (rad * 180) / Math.PI;

/** Feeds a layout to the scene, and draws the scene. */
export function SceneView({ layout, ctx, scene }: { layout: RingsLayout; ctx: MapCtx; scene: MapScene }) {
  // Targets: where everything should be; the scene eases there.
  useLayoutEffect(() => {
    scene.setTargets(
      { x: layout.cx, y: layout.cy },
      layout.rings.map((r) => ({ id: `ring-${r.ring}`, r: r.r })),
      layout.nodes.map((n) => {
        const data: SceneNode = {
          ...n,
          rel: { x: n.label.x - n.x, y: n.label.y - n.y },
          leaderRel: n.leader && [n.leader[0] - n.x, n.leader[1] - n.y, n.leader[2] - n.x, n.leader[3] - n.y],
        };
        return { id: n.clone.name, x: n.x, y: n.y, rel: data.rel, parent: n.parent, data };
      }),
      (id) => layout.hiddenInto.get(id),
    );
  }, [layout, scene]);
  useLayoutEffect(() => scene.setPeeling(ctx.peeling), [ctx.peeling, scene]);
  useLayoutEffect(() => scene.setBreathing(ctx.refreshing), [ctx.refreshing, scene]);
  // A new generation: its rims shift in. A failed refresh: its skin fades out.
  const lastGen = useRef(ctx.newest);
  useLayoutEffect(() => {
    if (ctx.newest > lastGen.current && !reducedMotion()) {
      scene.shiftAt = performance.now();
      scene.kick();
    }
    lastGen.current = ctx.newest;
  }, [ctx.newest, scene]);
  useLayoutEffect(() => {
    if (!ctx.refreshFailed) return;
    scene.failAt = performance.now();
    scene.kick();
  }, [ctx.refreshFailed, scene]);

  const members = useSyncExternalStore(scene.subscribe, scene.members);
  const parts = useRef(new Map<string, Parts>());
  const part = (id: string) => {
    let p = parts.current.get(id);
    if (!p) {
      p = {};
      parts.current.set(id, p);
    }
    return p;
  };
  const coreRef = useRef<SVGGElement>(null);
  /** The bulb's drawing: it breathes while the image refreshes. */
  const breathRef = useRef<SVGGElement>(null);
  /** The skin forming round the bulb during a refresh, and the label's animated dots. */
  const formingRef = useRef<SVGPathElement>(null);
  const formingLength = useRef(0);
  const dotsRef = useRef<SVGTSpanElement>(null);
  const inviteRef = useRef<SVGGElement>(null);
  const nudgeRef = useRef<SVGGElement>(null);
  /** The bulb's shape, for older clones' roots to reach their contour. */
  const shape = useRef(bulbShape(bulbSeed(ctx.repoName)));
  shape.current = useMemo(() => bulbShape(bulbSeed(ctx.repoName)), [ctx.repoName]);
  const ghostRef = useRef<SVGGElement>(null);
  const ghostStem = useRef<SVGPathElement>(null);
  const columnRef = useRef<SVGGElement>(null);
  /** The generation under the pointer (or focused): it and its clones' stems light up. */
  const [hoverAge, setHoverAge] = useState<number | null>(null);
  /** The clove hovered (else picked): its generation's tag shows at its root. */
  const cloveTagRef = useRef<SVGGElement>(null);
  const cloveFocus = useMemo(() => {
    const name = ctx.hovered ?? ctx.picked;
    let n = name ? layout.nodes.find((x) => x.clone.name === name) : undefined;
    if (!n) return null;
    const age = n.ring;
    // A fork roots through its lineage: the tag goes to its top-level clone's root.
    while (n?.parent) {
      const up: typeof n | undefined = layout.nodes.find((x) => x.clone.name === n?.parent);
      if (!up) break;
      n = up;
    }
    return { id: n.clone.name, age };
  }, [layout, ctx.hovered, ctx.picked]);
  const cloveFocusRef = useRef(cloveFocus);
  cloveFocusRef.current = cloveFocus;

  // The one writer: every frame, everything from the springs.
  const write = useMemo(
    () => (now: number) => {
      const cx = scene.core.x.v;
      const cy = scene.core.y.v;
      coreRef.current?.setAttribute("transform", `translate(${cx} ${cy})`);
      // Refreshing: the outer skin swells and settles, slowly.
      const breath = scene.breathing ? Math.sin((now / 1700) * Math.PI * 2) : 0;
      const breathScale = scene.breathing ? 1.01 + 0.01 * breath : 1;
      // A new generation settling: the new rim shrinks from where it formed onto the bulb...
      const sk = Math.min(1, (now - scene.shiftAt) / SHIFT_MS);
      const se = 1 - (1 - sk) ** 3;
      const shifting = sk < 1;
      breathRef.current?.setAttribute("transform", `scale(${shifting ? 1.08 + (1 - 1.08) * se : breathScale})`);
      breathRef.current?.style.setProperty("--breath", `${(breath + 1) / 2}`);
      columnRef.current?.setAttribute("transform", `translate(${cx} ${cy}) scale(${breathScale})`);
      // ...and each older rim moves in from where the one before it was.
      const shiftOf = (age: number) => {
        if (!shifting || age <= 0) return 1;
        const from = (1 - 0.2 * (age - 1)) / (1 - 0.2 * age);
        return from + (1 - from) * se;
      };
      for (const el of columnRef.current?.children ?? []) {
        const age = Number((el as SVGElement).dataset?.age ?? 0);
        if (age > 0) el.setAttribute("transform", `scale(${shiftOf(age)})`);
      }
      // The skin forming round the bulb: drawn round and round while the refresh runs.
      const fm = formingRef.current;
      if (fm) {
        formingLength.current ||= fm.getTotalLength?.() ?? 0;
        const L = formingLength.current;
        const failing = now - scene.failAt < FAIL_MS;
        if (scene.breathing && !reducedMotion()) {
          const t = (now % SWEEP_MS) / SWEEP_MS;
          fm.style.strokeDasharray = `${L} ${L}`;
          fm.style.strokeDashoffset = `${L * (1 - t)}`;
          fm.style.setProperty("--appear", "1");
        } else if (failing) {
          fm.style.strokeDashoffset = "0";
          fm.style.setProperty("--appear", `${1 - (now - scene.failAt) / FAIL_MS}`);
        } else fm.style.setProperty("--appear", "0");
      }
      // The invite: pinned to the bulb; only its ghost and stem nudge out every few seconds (further when the bulb is hovered).
      if (inviteRef.current && scene.inviting) {
        inviteRef.current.setAttribute("transform", `translate(${Math.round(cx)} ${Math.round(cy)})`);
        const beat = reducedMotion() ? 0.5 : (1 - Math.cos(((now % INVITE_MS) / INVITE_MS) * Math.PI * 2)) / 2;
        const out = (scene.bulbHover ? 22 : 8) * beat;
        nudgeRef.current?.setAttribute("transform", `translate(${INVITE_UX * out} ${INVITE_UY * out})`);
      }
      if (dotsRef.current) dotsRef.current.textContent = ".".repeat(1 + (Math.floor(now / 400) % 3));
      const cf = cloveFocusRef.current;
      const cat = cf ? scene.at(cf.id) : undefined;
      if (cloveTagRef.current && cf && cat) {
        const dir = Math.atan2(cat.y - cy, cat.x - cx);
        // Just inside its root's dot, so the dot stays visible.
        const r = contourRadius(shape.current, cf.age, dir) - 11;
        cloveTagRef.current.setAttribute("transform", `translate(${cx + Math.cos(dir) * r} ${cy + Math.sin(dir) * r})`);
      }
      const pull = scene.pull;
      for (const n of scene.nodes.values()) {
        const p = parts.current.get(n.id);
        if (!p) continue;
        const off = scene.pullOffset(n.id);
        // A flying clove sways across its path like a leaf.
        const flyT = n.fly ? Math.min(1, (now - n.fly.at) / FLY_MS) : 0;
        const sway = n.fly ? Math.sin(flyT * Math.PI * 3) * 26 * (1 - flyT * 0.4) * n.turn : 0;
        const x = n.x.v + off.dx + (n.fly ? -n.fly.uy * sway : 0);
        const y = n.y.v + off.dy + (n.fly ? n.fly.ux * sway : 0);
        const o = Math.max(0, Math.min(1, n.o.v));
        const dx = x - cx;
        const dy = y - cy;
        const len = Math.hypot(dx, dy);
        // Which way the clove points: away from the image, from where it is now.
        const dir = len > 4 ? Math.atan2(dy, dx) : n.data.angle;
        const spin = n.leaving === "fly" ? (1 - o) * 480 * n.turn : 0;
        const peel = scene.peeling.has(n.id) ? (now % 900) / 900 : null;
        const wobble = peel === null ? 0 : -24 * Math.sin(Math.PI * Math.min(1, peel / 0.8));
        p.g?.setAttribute("transform", `translate(${x} ${y}) scale(${Math.max(0.01, n.s.v)})`);
        p.g?.style.setProperty("--appear", `${o}`);
        p.rot?.setAttribute("transform", `rotate(${deg(dir) + 90 + spin + wobble})`);
        p.huddle?.setAttribute("transform", `rotate(${deg(dir)})`);
        if (p.skins) {
          // Rewinding peels the skins: outermost first, each lifts, slides off
          // along the clove's tip (local -y, the rotation turns it outward) and fades.
          const layers = [...p.skins.children] as SVGGElement[];
          layers.forEach((c, i) => {
            const sx = Number(c.dataset.sx);
            const sy = Number(c.dataset.sy);
            const order = layers.length - 1 - i;
            const t = peel === null ? 0 : Math.max(0, Math.min(1, (peel - order * 0.12) / 0.6));
            const lift = 1 + 0.12 * t;
            c.setAttribute("transform", `translate(0 ${-16 * t}) scale(${sx * lift} ${sy * lift})`);
            c.setAttribute("opacity", `${1 - t}`);
          });
        }
        // A label moving far fades out where it was, then in where it goes.
        const sw = n.swap ? Math.min(1, (now - n.swap.at) / SWAP_MS) : 1;
        const lrx = sw < 0.5 && n.swap ? n.swap.x : n.rx.v;
        const lry = sw < 0.5 && n.swap ? n.swap.y : n.ry.v;
        p.label?.setAttribute("transform", `translate(${x + lrx + n.lx.v} ${y + lry + n.ly.v})`);
        // Handed over from a drop: the name fades in once the clove has nearly arrived.
        const labelIn = n.labelAt === null ? 1 : Math.max(0, Math.min(1, (now - n.labelAt - 400) / LABEL_IN_MS));
        p.label?.style.setProperty("--appear", `${o * Math.abs(1 - 2 * sw) * labelIn}`);
        const lr = n.data.leaderRel;
        if (p.leader && lr) {
          const [line, dot] = p.leader.children;
          line.setAttribute("x1", `${x + lr[0]}`);
          line.setAttribute("y1", `${y + lr[1]}`);
          line.setAttribute("x2", `${x + lr[2]}`);
          line.setAttribute("y2", `${y + lr[3]}`);
          dot.setAttribute("cx", `${x + lr[2]}`);
          dot.setAttribute("cy", `${y + lr[3]}`);
          p.leader.style.setProperty("--appear", `${n.lx.v || n.ly.v ? 0 : o}`);
        }
        if (p.badge) {
          const bx = x - Math.sin(dir) * 24;
          const by = y + Math.cos(dir) * 24;
          p.badge.setAttribute("transform", `translate(${bx} ${by})`);
          p.badge.style.setProperty("--appear", `${o}`);
        }
        if (p.edge) {
          // A flying leaf has let go of its stem; a rip holds on to it until it's let go.
          let d = "";
          let recoil = false;
          if (n.fly) {
            // Snapped: the stem springs back to its root, with a little wobble.
            const r = (now - n.fly.at) / RECOIL_MS;
            if (r < 1) {
              recoil = true;
              const k = (1 - (1 - r) ** 3) * (1 + 0.15 * Math.sin(r * Math.PI * 3) * (1 - r));
              const ex = n.fly.x + (n.fly.rootX - n.fly.x) * Math.min(1.05, k);
              const ey = n.fly.y + (n.fly.rootY - n.fly.y) * Math.min(1.05, k);
              if (Math.hypot(ex - n.fly.rootX, ey - n.fly.rootY) > 2) d = stemPath(n.fly.rootX, n.fly.rootY, ex, ey);
            }
          } else {
            const from = n.data.parent ? scene.at(n.data.parent) : undefined;
            if (from) d = stemPath(from.x, from.y, x, y);
            else {
              const e = edgeGeom(cx, cy, x, y, n.data.ring);
              if (e) d = quadPath(e);
            }
          }
          p.edge.setAttribute("d", d);
          p.edge.style.setProperty("--appear", `${recoil ? 1 : o}`);
          p.edge.classList.toggle("map-edge-recoil", recoil);
        }
        if (p.root) {
          const [path, tip] = p.root.children;
          const ux = len ? dx / len : 0;
          const uy = len ? dy / len : 0;
          const r = contourRadius(shape.current, n.data.ring, dir) * shiftOf(n.data.ring);
          path.setAttribute("d", `M ${cx + ux * r} ${cy + uy * r} L ${cx + ux * CORE_RADIUS} ${cy + uy * CORE_RADIUS}`);
          tip.setAttribute("cx", `${cx + ux * r}`);
          tip.setAttribute("cy", `${cy + uy * r}`);
          p.root.style.setProperty("--appear", `${n.leaving === "fly" ? 0 : o}`);
        }
      }
      // The pulled copy (a fork's) or sprout (the image's).
      const g = scene.ghost;
      const gp = g.pull;
      if (ghostRef.current && gp) {
        const live = pull === gp;
        let at = live ? scene.ghostAt(gp) : { x: g.x.v, y: g.y.v };
        const home = scene.ghostHome(gp);
        let stem = "";
        if (gp.target.kind === "image") {
          // It sits at least on the image's rim, seen coming out of it, on an image edge's sweep.
          const ddx = at.x - cx;
          const ddy = at.y - cy;
          const l = Math.hypot(ddx, ddy) || 1;
          const out = Math.max(l, CORE_RADIUS - 8);
          at = { x: cx + (ddx / l) * out, y: cy + (ddy / l) * out };
          const e = edgeGeom(cx, cy, at.x, at.y, 0);
          if (e && out > CORE_RADIUS + 4) stem = quadPath(e);
        } else if (Math.hypot(at.x - home.x, at.y - home.y) > 20) {
          stem = stemPath(home.x, home.y, at.x, at.y);
        }
        const base = gp.target.kind === "image" ? 0.35 + 0.65 * gp.strain : 0.55 + 0.45 * gp.strain;
        const scale = live ? base : base * Math.max(0.2, g.s.v);
        const dir = Math.atan2(at.y - cy, at.x - cx);
        ghostRef.current.setAttribute("transform", `translate(${at.x} ${at.y})`);
        ghostRef.current.style.setProperty("--appear", `${Math.max(0, Math.min(1, g.o.v))}`);
        const [body] = ghostRef.current.children;
        body?.setAttribute("transform", `scale(${scale}) rotate(${deg(dir) + 90})`);
        ghostStem.current?.setAttribute("d", stem);
        ghostStem.current?.style.setProperty("--appear", `${Math.max(0, Math.min(1, g.o.v))}`);
      }
    },
    [scene],
  );
  useLayoutEffect(() => scene.onFrame(write), [scene, write]);
  // New elements get their place before they're painted.
  useLayoutEffect(() => write(performance.now()));

  // The live pull first: the scene hears of it only after this render.
  // An image but no clones (and nothing being pulled): invite the first pull.
  const inviting = members.length === 0 && ctx.current !== undefined && !ctx.pull;
  scene.inviting = inviting;
  const ghost = ctx.pull && (ctx.pull.target.kind === "image" || ctx.pull.fork) ? ctx.pull : scene.ghost.pull;
  const byName = new Map(members.map((m) => [m.id, m]));
  return (
    <svg className="map-scene absolute top-0 left-0 overflow-visible" width={layout.width} height={layout.height}>
      <g>
        {members.map((m) => (
          <path
            key={`edge-${m.id}`}
            data-for={m.id}
            ref={(el) => {
              part(m.id).edge = el;
            }}
            fill="none"
            style={{ "--strain": pullOn(ctx, m.id)?.strain ?? 0 } as CSSProperties}
            className={cn(
              "map-edge",
              m.data.parent && byName.has(m.data.parent) && "map-stem",
              pullOn(ctx, m.id) && "map-edge-strained",
              pullOn(ctx, m.id)?.torn && "map-edge-torn",
              m.data.ring > 0 && "map-edge-old",
              (ctx.picked === m.id || ctx.hovered === m.id || hoverAge === m.data.ring) && "map-edge-active",
              isDimmed(ctx, m.data.clone.golden_generation, m.id) && "map-dim",
            )}
          />
        ))}
        {members.map(
          (m) =>
            m.data.leaderRel && (
              // Dotted, with a dot at the name's end: never read as a stem.
              <g
                key={`leader-${m.id}`}
                ref={(el) => {
                  part(m.id).leader = el;
                }}
                className={cn("map-leader", isDimmed(ctx, m.data.clone.golden_generation, m.id) && "map-dim")}
              >
                <line />
                <circle r={1.6} />
              </g>
            ),
        )}
      </g>
      <Core ctx={ctx} scene={scene} coreRef={coreRef} breathRef={breathRef} formingRef={formingRef} dotsRef={dotsRef} />
      {members.map(
        (m) =>
          !m.data.parent && (
            <g
              key={`root-${m.id}`}
              ref={(el) => {
                part(m.id).root = el;
              }}
              className={cn(
                "map-root",
                hoverAge === m.data.ring && "map-root-lit",
                isDimmed(ctx, m.data.clone.golden_generation, m.id) && "map-dim",
              )}
            >
              {/* It roots on its generation's contour, inside the bulb. */}
              <path
                className={cn(
                  "map-edge",
                  m.data.ring > 0 && "map-edge-old",
                  (ctx.picked === m.id || ctx.hovered === m.id || hoverAge === m.data.ring) && "map-edge-active",
                )}
              />
              <circle r={2} className="map-root-tip" />
            </g>
          ),
      )}
      {ghost && (
        <g>
          <path
            ref={ghostStem}
            fill="none"
            className={cn(
              "map-sprout-edge",
              ghost.target.kind === "clone" && "map-fork-stem",
              ghost.torn && "map-sprout-edge-free",
            )}
          />
          <g
            ref={ghostRef}
            className={cn(
              "map-ghost map-piece map-skin-peeled",
              ghost.fork ? "map-fork-copy" : "map-sprout",
              ghost.torn && "map-ghost-free",
            )}
          >
            <g>
              <circle r={SYMBOL} className="map-ghost-halo" />
              <CloveShape length={30} />
            </g>
            {(ghost.torn || ghost.held) && (
              <text className="map-sprout-hint" y={34} textAnchor="middle">
                {ghost.fork
                  ? ghost.held
                    ? "forking…"
                    : "let go to fork"
                  : ghost.held
                    ? "growing…"
                    : "let go for a new clone"}
              </text>
            )}
          </g>
        </g>
      )}
      {inviting && (
        // No clones yet: a ghost clove peeks out of the bulb, inviting a pull.
        // Only the ghost and its stem nudge (a transform on their group); the
        // caption is placed once, at whole pixels, beside the ghost's rest place.
        <g ref={inviteRef} className="map-invite">
          <g ref={nudgeRef}>
            <path
              className="map-invite-stem"
              d={`M ${INVITE_UX * (BULB_R - 2)} ${INVITE_UY * (BULB_R - 2)} L ${INVITE_UX * (INVITE_REST - 14)} ${INVITE_UY * (INVITE_REST - 14)}`}
            />
            <g
              className="map-invite-clove"
              transform={`translate(${INVITE_UX * INVITE_REST} ${INVITE_UY * INVITE_REST})`}
            >
              <circle r={SYMBOL} className="map-invite-halo" />
              <g transform={`rotate(${(INVITE_ANGLE * 180) / Math.PI + 90})`}>
                <CloveShape length={28} />
              </g>
            </g>
          </g>
          <text
            className="map-invite-caption"
            x={Math.round(INVITE_UX * INVITE_REST + 24)}
            y={Math.round(INVITE_UY * INVITE_REST - 4)}
          >
            <tspan className="map-invite-title">Pull a clove out of the bulb</tspan>
            <tspan x={Math.round(INVITE_UX * INVITE_REST + 24)} dy={15}>
              to make a clone · or press {MOD}N
            </tspan>
          </text>
        </g>
      )}
      {members.map((m) => (
        <Clove key={m.id} m={m.data} leaving={!!m.leaving} ctx={ctx} parts={part(m.id)} />
      ))}
      {[...members.filter((m) => m.id !== ctx.hovered), ...members.filter((m) => m.id === ctx.hovered)].map((m) => (
        <Label
          key={`label-${m.id}`}
          m={m.data}
          ctx={ctx}
          leaving={!!m.leaving}
          labelRef={(el) => {
            part(m.id).label = el;
          }}
        />
      ))}
      {members.map((m) => {
        const bulb = layout.bulbs.get(m.id);
        if (!bulb || m.leaving || pullOn(ctx, m.id) || forkPullOn(ctx, m.id)) return null;
        return (
          <g
            key={`bulb-${m.id}`}
            ref={(el) => {
              part(m.id).badge = el;
            }}
            role="button"
            tabIndex={0}
            aria-expanded={bulb.open}
            aria-label={bulb.open ? `Fold ${m.id}'s forks` : `Show ${bulb.forks} forks of ${m.id}`}
            className={cn(
              "map-bulb-badge",
              bulb.open && "map-bulb-open",
              isDimmed(ctx, m.data.clone.golden_generation, m.id) && "map-dim",
            )}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              ctx.toggleBulb(m.id);
            }}
            onKeyDown={(e) => {
              if (e.key !== "Enter" && e.key !== " ") return;
              e.preventDefault();
              e.stopPropagation();
              ctx.toggleBulb(m.id);
            }}
          >
            <rect x={-12} y={-8} width={24} height={16} rx={8} />
            <text textAnchor="middle" y={3.5}>
              {bulb.open ? "−" : `+${bulb.forks}`}
            </text>
          </g>
        );
      })}
      {
        <LayerButtons
          layout={layout}
          ctx={ctx}
          shape={shape.current}
          hover={hoverAge}
          cloveAge={cloveFocus?.age ?? null}
          onHover={setHoverAge}
          layersRef={columnRef}
          cloveTagRef={cloveTagRef}
        />
      }
    </svg>
  );
}

/**
 * Checkpoint skins: the clove's outline grown outward by an even gap per layer
 * (2px, then 1.8px more each), scaled separately across and along the clove
 * so the gap is the same at its sides, tip and base.
 */
function skinScales(length: number, n: number): [number, number][] {
  // The clove path spans about 40 units across and 82 along, of 88.
  const w = (length * 40) / 88;
  const h = (length * 82) / 88;
  return Array.from({ length: n }, (_, i) => {
    const d = 2 + i * 1.8;
    return [Number(((w + 2 * d) / w).toFixed(4)), Number(((h + 2 * d) / h).toFixed(4))];
  });
}

/** The clove shape, tip up, `length` long, centred on 0,0. */
function CloveShape({ length }: { length: number }) {
  return (
    <g className="map-piece-svg" transform={`scale(${length / 88}) translate(-50 -48)`}>
      <path className="clove-skin" d={CLOVE_PATH} strokeLinejoin="round" />
      <path className="clove-vein" d="M50 24 C46 44 46 64 50 80" fill="none" />
    </g>
  );
}

/** A pulled clove turns rose as it strains: its halo, inline so no skin rule outranks it. */
function pullTint(strain: number): CSSProperties {
  return {
    fill: `color-mix(in oklch, var(--rose) ${Math.round(strain * 22)}%, var(--bottle-soft))`,
    stroke: `color-mix(in oklch, var(--rose) ${Math.round(strain * 30)}%, transparent)`,
    strokeWidth: 8 + strain * 8,
  };
}

/** A clone on the map: a focusable group centred on its place (the frame loop moves it). */
function Clove({ m, leaving, ctx, parts }: { m: SceneNode; leaving: boolean; ctx: MapCtx; parts: Parts }) {
  const c = m.clone;
  const pr = ctx.prOf(c.name);
  const pull = pullOn(ctx, c.name);
  const warning = !!(c.failed_steps.length || c.issue || c.operation);
  const count = c.checkpoints ?? 0;
  const length = Math.max(26, Math.min(32, cloveLength(c)));
  return (
    <g
      ref={(el) => {
        parts.g = el;
      }}
      role="button"
      tabIndex={leaving ? -1 : 0}
      aria-hidden={leaving || undefined}
      aria-label={`Clone ${c.name}`}
      aria-pressed={ctx.picked === c.name}
      {...menuTrigger(ctx, { kind: "clone", name: c.name }, () => ctx.select({ kind: "clone", name: c.name }))}
      onPointerDown={(event) => !leaving && ctx.startPull({ kind: "clone", name: c.name }, event)}
      onPointerEnter={() => ctx.setHovered(() => c.name)}
      onFocus={() => ctx.setHovered(() => c.name)}
      onBlur={() => ctx.setHovered((h) => (h === c.name ? null : h))}
      onPointerLeave={() => ctx.setHovered((h) => (h === c.name ? null : h))}
      style={leaving ? { pointerEvents: "none" } : undefined}
      className={cn(
        "map-clone map-node",
        ctx.picked === c.name && "map-node-selected",
        ctx.menuFor === c.name && "map-menu-open",
        pull && "map-pulling",
        pull?.torn && "map-pull-torn",
        isDimmed(ctx, c.golden_generation, c.name) && "map-dim",
        !c.mounted && "map-clone-unmounted",
        warning && "map-clone-warning",
        // Roots: an older image shows as a drier skin, one step per image.
        m.ring > 0 && `map-age map-age-${Math.min(m.ring, MAX_AGE)}`,
        skinClasses(c, pr),
        ctx.peeling.has(c.name) && "map-peeling",
        forkPullOn(ctx, c.name) && "map-forking",
      )}
    >
      <circle r={SYMBOL + 5} className="map-focus-ring" />
      {m.folded > 0 && !leaving && (
        // Small cloves huddled behind it: its folded forks. +x is outward.
        <g
          className="map-bulb-huddle"
          ref={(el) => {
            parts.huddle = el;
          }}
        >
          {(
            [
              [-0.6, 13],
              [0.6, 13],
              [0, 18],
            ] as const
          ).map(([side, out]) => (
            <g key={`${side}-${out}`} transform={`translate(${out} ${side * 11}) rotate(${90 + side * 25})`}>
              <CloveShape length={16} />
            </g>
          ))}
        </g>
      )}
      <circle r={SYMBOL} className="map-clone-halo" style={pull ? pullTint(pull.strain) : undefined} />
      {count > 3 && (
        <g className="map-skin-count" transform="translate(21 21)">
          <rect x={-8} y={-6.5} width={16} height={13} rx={6.5} />
          <text textAnchor="middle" y={3}>
            {count}
          </text>
        </g>
      )}
      <g
        ref={(el) => {
          parts.rot = el;
        }}
      >
        {/* Checkpoints: papery skins following the clove's own outline, turned with it. */}
        <g
          className="map-skins"
          ref={(el) => {
            parts.skins = el;
          }}
        >
          {skinScales(length, Math.min(3, count)).map(([sx, sy], i) => (
            <g
              key={`${sx}-${sy}`}
              className="map-skin-layer"
              data-sx={sx}
              data-sy={sy}
              transform={`scale(${sx} ${sy})`}
              style={{ "--layer": i } as CSSProperties}
            >
              <path
                d={CLOVE_PATH}
                transform={`scale(${length / 88}) translate(-50 -48)`}
                vectorEffect="non-scaling-stroke"
              />
            </g>
          ))}
        </g>
        {/* Size follows the clone's own data, kept inside the 34px target. */}
        <CloveShape length={length} />
      </g>
    </g>
  );
}

/** A name, cut to fit with an ellipsis. */
function fit(text: string, width: number, font: string) {
  if (textWidth(text, font) <= width) return text;
  let s = text;
  while (s.length > 1 && textWidth(`${s}…`, font) > width) s = s.slice(0, -1);
  return `${s}…`;
}

/** A clone's name and state line, at its layout spot; the frame loop keeps it on its clove. */
function Label({
  m,
  ctx,
  leaving,
  labelRef,
}: {
  m: SceneNode;
  ctx: MapCtx;
  leaving: boolean;
  labelRef: (el: SVGGElement | null) => void;
}) {
  const c = m.clone;
  const { w, align } = m.label;
  const lifted = ctx.menuFor === c.name;
  // Lifted over its menu, it centres above the ring.
  const anchor = lifted ? "center" : align;
  const nameFont = `500 12px ${SANS}`;
  // Cut to fit; while its clove is hovered or focused, the full name shows in place (over its neighbours).
  const expanded = ctx.hovered === c.name;
  const name = expanded ? c.name : fit(c.name, w - 12, nameFont);
  const nameW = textWidth(name, nameFont) + 12;
  const parts = cloneState(c, ctx.prOf(c.name));
  const stateText = (p: StatePart) => `${p.arrow === "up" ? "↑" : p.arrow === "down" ? "↓" : ""}${p.text}`;
  const stateW =
    parts.reduce((n, p) => n + textWidth(stateText(p), `10.5px ${SANS}`), 0) + 8 * Math.max(0, parts.length - 1) + 12;
  const x = (bw: number) => (anchor === "left" ? 0 : anchor === "right" ? w - bw : (w - bw) / 2);
  const textAnchor = anchor === "left" ? "start" : anchor === "right" ? "end" : "middle";
  // Both lines share one anchor and x, 6px inside their pills, so their text edges line up.
  const tx = anchor === "left" ? 6 : anchor === "right" ? w - 6 : w / 2;
  return (
    <g
      ref={labelRef}
      data-for={c.name}
      className={cn(
        "map-label",
        ctx.picked === c.name && "map-node-selected",
        lifted && "map-menu-open",
        isDimmed(ctx, c.golden_generation, c.name) && "map-dim",
        pullOn(ctx, c.name)?.torn && "map-pull-torn",
        leaving && "map-leaving",
        expanded && name !== fit(c.name, w - 12, nameFont) && "map-label-expanded",
      )}
    >
      <rect className="map-clone-name-bg" x={x(nameW)} y={0} width={nameW} height={15} rx={4} />
      <text className="map-clone-name" x={tx} y={LABEL_NAME_Y} textAnchor={textAnchor}>
        {name}
      </text>
      <g className="map-clone-state">
        <rect className="map-clone-state-bg" x={x(stateW)} y={LABEL_STATE_Y - 8} width={stateW} height={11.5} />
        <text x={tx} y={LABEL_STATE_Y} textAnchor={textAnchor}>
          {parts.map((p, i) => (
            <tspan
              key={`${p.text}-${p.arrow ?? ""}`}
              dx={i ? 8 : 0}
              className={p.tone ? `map-tone-${p.tone}` : undefined}
            >
              {stateText(p)}
            </tspan>
          ))}
        </text>
      </g>
    </g>
  );
}

/** A repo's own bulb: the drawing is seeded by its name, so it stays the same across refreshes. */
export const bulbSeed = (name: string) => [...name].reduce((h, ch) => (h * 31 + ch.charCodeAt(0)) >>> 0, 7);

/**
 * The golden image as a garlic bulb seen from below, in line art (map/bulb.tsx),
 * with the repo's name and image on a small pill under its rim.
 */
function Core({
  ctx,
  scene,
  coreRef,
  breathRef,
  formingRef,
  dotsRef,
}: {
  ctx: MapCtx;
  scene: MapScene;
  coreRef: React.RefObject<SVGGElement | null>;
  breathRef: React.RefObject<SVGGElement | null>;
  formingRef: React.RefObject<SVGPathElement | null>;
  dotsRef: React.RefObject<SVGTSpanElement | null>;
}) {
  const selected = ctx.imageGeneration === ctx.current;
  const base = ctx.base.replace(/^[^/]+\//, "");
  // A quiet "behind" note when the image lags its base (the inspector bar used to say it).
  const gen = `gen ${ctx.newest}${base ? ` · ${base}` : ""}${ctx.behind ? ` · ${ctx.behind} behind` : ""}`;
  return (
    <g
      ref={coreRef}
      role="button"
      tabIndex={0}
      aria-label={`Bulb (golden image), generation ${ctx.newest}`}
      aria-pressed={selected}
      className={cn(
        "map-image map-node",
        selected && "map-node-selected",
        ctx.pull?.target.kind === "image" && "map-image-pulled",
        ctx.refreshing && "map-bulb-refreshing",
      )}
      onPointerDown={(event) => ctx.current !== undefined && ctx.startPull({ kind: "image" }, event)}
      onPointerEnter={() => {
        scene.bulbHover = true;
        scene.kick();
      }}
      onPointerLeave={() => {
        scene.bulbHover = false;
      }}
      {...menuTrigger(
        ctx,
        { kind: "image" },
        () => ctx.current !== undefined && ctx.select({ kind: "image", generation: ctx.current }),
      )}
    >
      <BulbArt seed={bulbSeed(ctx.repoName)} deepest={ctx.deepestAge} breathRef={breathRef} />
      {/* A new skin forming just outside the rim while the image refreshes (drawn round by the frame loop). */}
      <path
        ref={formingRef}
        d={rimPath(bulbSeed(ctx.repoName))}
        transform="scale(1.08)"
        vectorEffect="non-scaling-stroke"
        className={cn("map-bulb-forming", ctx.refreshFailed && "map-bulb-forming-failed")}
      />
      {/* The repo and its image, two lines under the bulb; a halo of the page colour keeps stems from cutting through. */}
      <g className="map-bulb-label" transform={`translate(0 ${REPO_LABEL_Y})`}>
        <text className="map-bulb-name" y={BULB_NAME_Y} textAnchor="middle">
          {ctx.repoName}
        </text>
        <text className="map-bulb-gen" y={BULB_GEN_Y} textAnchor="middle">
          {ctx.refreshing ? (
            <>
              refreshing{ctx.refreshStep ? ` · ${ctx.refreshStep}` : ""}
              <tspan ref={dotsRef} />
            </>
          ) : ctx.refreshFailed ? (
            <tspan className="map-bulb-failed">refresh failed</tspan>
          ) : (
            gen
          )}
        </text>
      </g>
    </g>
  );
}

/** A generation's short tag: "12", or "≤11" for the oldest group. */
const genTag = (l: { age: number; generations: number[] }) =>
  l.age === MAX_AGE && l.generations.length > 1 ? `≤${l.generations[0]}` : `${l.generations[0]}`;

/**
 * The image's generations as buttons, pinned to the bulb by the
 * frame loop. The current one is the rim; older ones are contours inside it,
 * deeper and dashed. A generation's number shows only while it's lit: hovered
 * or focused, picked, or the generation of the clove hovered or picked (then
 * at that clove's root, see cloveTagRef).
 */
function LayerButtons({
  layout,
  ctx,
  shape,
  hover,
  cloveAge,
  onHover,
  layersRef,
  cloveTagRef,
}: {
  layout: RingsLayout;
  ctx: MapCtx;
  shape: BulbShape;
  hover: number | null;
  cloveAge: number | null;
  onHover: (age: number | null) => void;
  layersRef: React.RefObject<SVGGElement | null>;
  cloveTagRef: React.RefObject<SVGGElement | null>;
}) {
  const { layers } = layout;
  const paths = useMemo(() => new Map(layers.map((l) => [l.age, contourPath(shape, l.age)])), [layers, shape]);
  const font = `600 9px ${SANS}`;
  const cloveLayer = layers.find((l) => l.age === cloveAge);
  const cloveW = cloveLayer ? textWidth(genTag(cloveLayer), font) + 8 : 0;
  return (
    <>
      <g ref={layersRef} role="group" aria-label="Image generations" className="map-layer-buttons">
        {layers.map((l, i) => {
          const gen = l.generations;
          const full =
            l.age === MAX_AGE && gen.length > 1 ? `gen ${gen[gen.length - 1]}–${gen[0]}` : `gen ${gen.join(", ")}`;
          const picked = ctx.focusAge === l.age;
          const lit = hover === l.age || picked || (hover === null && cloveAge === l.age);
          const pick = () => ctx.select({ kind: "image", generation: l.age === 0 ? (ctx.current ?? gen[0]) : gen[0] });
          // Its tag near the top of its contour, spread so neighbours' don't stack.
          const a = -Math.PI / 2 - 0.3 + i * 0.3;
          const r = contourRadius(shape, l.age, a);
          const short = genTag(l);
          const w = textWidth(short, font) + 8;
          return (
            <g
              key={l.age}
              data-age={l.age}
              role="button"
              tabIndex={0}
              aria-pressed={picked}
              aria-label={l.age === 0 ? `Clones from the current image, ${full}` : `Clones from older image, ${full}`}
              className={cn(
                "map-layer-button",
                l.age === 0 ? "map-layer-rim" : "map-layer-old",
                lit && "map-layer-lit",
                picked && "map-node-selected",
              )}
              onPointerDown={(e) => e.stopPropagation()}
              onPointerEnter={() => onHover(l.age)}
              onPointerLeave={() => onHover(null)}
              onFocus={() => onHover(l.age)}
              onBlur={() => onHover(null)}
              onClick={(e) => {
                e.stopPropagation();
                pick();
              }}
              onKeyDown={(e) => {
                if (e.key !== "Enter" && e.key !== " ") return;
                e.preventDefault();
                e.stopPropagation();
                pick();
              }}
            >
              <path d={paths.get(l.age)} className="map-bulb-contour-hit" />
              <path d={paths.get(l.age)} className="map-bulb-contour" />
              {(hover === l.age || picked) && (
                <g className="map-layer-tag" transform={`translate(${Math.cos(a) * r} ${Math.sin(a) * r})`}>
                  <rect x={-w / 2} y={-6.5} width={w} height={13} rx={6.5} />
                  <text y={3.2} textAnchor="middle">
                    {short}
                  </text>
                </g>
              )}
            </g>
          );
        })}
      </g>
      {/* The hovered or picked clove's generation, at its root on the contour. */}
      {cloveLayer && hover === null && ctx.focusAge === null && (
        <g ref={cloveTagRef} className="map-layer-tag map-layer-tag-clove">
          <rect x={-cloveW / 2} y={-6.5} width={cloveW} height={13} rx={6.5} />
          <text y={3.2} textAnchor="middle">
            {genTag(cloveLayer)}
          </text>
        </g>
      )}
    </>
  );
}

const reducedMotion = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/** The empty map's invite: where the ghost clove peeks out (up and to the right), and its nudge's period. */
const INVITE_ANGLE = -Math.PI / 4;
const INVITE_UX = Math.cos(INVITE_ANGLE);
const INVITE_UY = Math.sin(INVITE_ANGLE);
/** The ghost's rest place, out from the bulb's centre. */
const INVITE_REST = BULB_R + 10;
const INVITE_MS = 3000;
