// The map's one animation clock. Every moving thing on the map is a spring
// here (cloves, the core, ring radii, a pulled copy, a label lifted over its
// menu); one requestAnimationFrame loop steps them and calls the writers the
// SVG registered, which set attributes directly. React renders the scene's
// structure only when what's on it changes, never per frame.

import type { Pull } from "./pull";

/** A value that eases toward its target: critically damped, a touch under. */
type Spring = { v: number; vel: number; to: number };
const spring = (v: number, to = v): Spring => ({ v, vel: 0, to });
const STIFF = 190;
/** A new generation settling in (the new rim shrinks onto the bulb, the old one moves inward). */
export const SHIFT_MS = 800;
/** A failed refresh's forming skin fading out. */
export const FAIL_MS = 1400;
/** The forming skin's sweep round the bulb. */
export const SWEEP_MS = 1800;
/** A clove handed over from a drop: its label fades in over this, near the end of its glide. */
export const LABEL_IN_MS = 350;
/** A ripped clove's flight, from letting go to gone. */
export const FLY_MS = 1000;
/** Its snapped stem springing back to its root. */
export const RECOIL_MS = 350;
/** The rest of the map closes the gap once the clove is this far through its flight. */
const REFLOW_AFTER = 0.5;

function step(s: Spring, dt: number, stiff = STIFF) {
  const damp = 2 * Math.sqrt(stiff) * 0.86;
  const a = stiff * (s.to - s.v) - damp * s.vel;
  s.vel += a * dt;
  s.v += s.vel * dt;
}
const settled = (s: Spring, eps = 0.05) => Math.abs(s.to - s.v) < eps && Math.abs(s.vel) < eps;
const snap = (s: Spring, v: number) => {
  s.v = v;
  s.to = v;
  s.vel = 0;
};

/** What the layout says about one clove; positions are absolute world units. */
export type SceneTarget<D> = {
  id: string;
  x: number;
  y: number;
  /** Its label's spot, relative to the clove: re-placed labels glide there. */
  rel: { x: number; y: number };
  /** Where a new one grows from: its parent's id, or null for the core. */
  parent: string | null;
  data: D;
};

export type NodeState<D> = {
  id: string;
  x: Spring;
  y: Spring;
  s: Spring;
  o: Spring;
  /** Handed over from a dropped copy: its label fades in near the end of the glide (from this time). */
  labelAt: number | null;
  /** Its label's offset from the clove (the layout's spot, eased when it changes). */
  rx: Spring;
  ry: Spring;
  /** A label moving far: it fades out where it was and in where it goes (`at` = start time). */
  swap: { x: number; y: number; at: number } | null;
  /** Label lift (world units, relative to the label's own spot), for an open menu. */
  lx: Spring;
  ly: Spring;
  data: D;
  /** The clove it hangs from on the map (a fork's parent), or null for the image. */
  parent: string | null;
  leaving: false | "fly" | "fold" | "gone";
  /** Flying off: when it started, from where, which way, and the root its stem recoils to. */
  fly: { at: number; x: number; y: number; ux: number; uy: number; rootX: number; rootY: number } | null;
  /** For a leaf flying off: which way it spins. */
  turn: number;
};

export type Ghost = {
  x: Spring;
  y: Spring;
  s: Spring;
  o: Spring;
  /** The pull it shows, or the last one while it fades or shrinks back. */
  pull: Pull | null;
};

type Writer = (now: number) => void;

export class Scene<D> {
  nodes = new Map<string, NodeState<D>>();
  core = { x: spring(0), y: spring(0) };
  rings = new Map<string, Spring>();
  ghost: Ghost = { x: spring(0), y: spring(0), s: spring(0), o: spring(0), pull: null };
  pull: Pull | null = null;
  /** The cloves a rip moves: the pulled one and its lineage. */
  private pullGroup = new Set<string>();
  /** Ripped lineages' shared way out, by clove. */
  private flyDir = new Map<string, { x: number; y: number }>();
  /** Clones being rewound: their skins peel back, on this clock. */
  peeling: ReadonlySet<string> = new Set();
  private writers = new Set<Writer>();
  private subs = new Set<() => void>();
  private version = 0;
  private list: NodeState<D>[] = [];
  private raf = 0;
  private last = 0;
  private first = true;

  // ---- structure, for React -----------------------------------------------

  subscribe = (cb: () => void) => {
    this.subs.add(cb);
    return () => this.subs.delete(cb);
  };
  /** The cloves on the map, leaving ones included; a new array only when that changes. */
  members = () => this.list;
  getVersion = () => this.version;
  private changed() {
    this.version++;
    this.list = [...this.nodes.values()];
    for (const s of this.subs) s();
  }

  /** Called every frame (and once when registered) until unregistered. */
  onFrame(w: Writer) {
    this.writers.add(w);
    w(performance.now());
    return () => {
      this.writers.delete(w);
    };
  }

  // ---- targets --------------------------------------------------------------

  /**
   * Moves everything toward a new layout. New cloves grow from their parent's
   * current place (a fork from its clove, a clone from the core); ones gone
   * from the layout fold back into `foldInto(id)` or fly off outward.
   */
  setTargets(
    core: { x: number; y: number },
    rings: { id: string; r: number }[],
    targets: SceneTarget<D>[],
    foldInto: (id: string) => string | undefined,
  ) {
    // Ripped cloves start flying at once; the rest of the map waits until
    // they're half gone, then closes the gap.
    const now = performance.now();
    if (this.startFlights(targets, foldInto, now)) this.deferUntil = now + FLY_MS * REFLOW_AFTER;
    if (now < this.deferUntil) {
      this.deferred = [core, rings, targets, foldInto];
      this.changed();
      this.kick();
      return;
    }
    this.applyTargets(core, rings, targets, foldInto);
  }

  private deferUntil = 0;
  private deferred: Parameters<Scene<D>["applyTargets"]> | null = null;

  /** Cloves gone from the layout and not folding away: they fly off now. Returns how many. */
  private startFlights(targets: SceneTarget<D>[], foldInto: (id: string) => string | undefined, now: number) {
    if (this.first) return 0;
    const ids = new Set(targets.map((t) => t.id));
    let n = 0;
    for (const c of this.nodes.values()) {
      if (ids.has(c.id) || c.leaving) continue;
      const into = foldInto(c.id);
      if (into && this.nodes.has(into)) continue;
      // A ripped lineage flies off together, one way; anything else, straight out.
      const group = this.flyDir.get(c.id);
      const dx = group ? group.x : c.x.v - this.core.x.v;
      const dy = group ? group.y : c.y.v - this.core.y.v;
      const len = Math.hypot(dx, dy) || 1;
      const parent = c.parent ? this.at(c.parent) : undefined;
      const cl = Math.hypot(c.x.v - this.core.x.v, c.y.v - this.core.y.v) || 1;
      c.leaving = "fly";
      c.fly = {
        at: now,
        x: c.x.v,
        y: c.y.v,
        ux: dx / len,
        uy: dy / len,
        rootX: parent ? parent.x : this.core.x.v + ((c.x.v - this.core.x.v) / cl) * 78,
        rootY: parent ? parent.y : this.core.y.v + ((c.y.v - this.core.y.v) / cl) * 78,
      };
      this.flyDir.delete(c.id);
      n++;
    }
    return n;
  }

  private applyTargets(
    core: { x: number; y: number },
    rings: { id: string; r: number }[],
    targets: SceneTarget<D>[],
    foldInto: (id: string) => string | undefined,
  ) {
    const first = this.first;
    this.first = false;
    // The first layout shows as is.
    if (first) {
      snap(this.core.x, core.x);
      snap(this.core.y, core.y);
    } else {
      this.core.x.to = core.x;
      this.core.y.to = core.y;
    }
    for (const r of rings) {
      const s = this.rings.get(r.id);
      if (s) s.to = r.r;
      else this.rings.set(r.id, first ? spring(r.r) : spring(0, r.r));
    }
    let structural = false;
    const seen = new Set<string>();
    for (const t of targets) {
      seen.add(t.id);
      const n = this.nodes.get(t.id);
      if (n) {
        if (n.leaving) structural = true;
        n.leaving = false;
        n.fly = null;
        n.x.to = t.x;
        n.y.to = t.y;
        n.s.to = 1;
        n.o.to = 1;
        // Far: fade across. Near: glide.
        if (Math.hypot(t.rel.x - n.rx.to, t.rel.y - n.ry.to) > SWAP_PX) {
          n.swap = { x: n.rx.v, y: n.ry.v, at: performance.now() };
          snap(n.rx, t.rel.x);
          snap(n.ry, t.rel.y);
        } else {
          n.rx.to = t.rel.x;
          n.ry.to = t.rel.y;
        }
        n.parent = t.parent;
        if (n.data !== t.data) structural = true;
        n.data = t.data;
        continue;
      }
      structural = true;
      const from = (t.parent && this.nodes.get(t.parent)) || null;
      // The clone a dropped sprout or fork copy was waiting for: it starts where that was dropped.
      const pend = this.pending;
      const handoff = !first && !!pend && (pend.parent ?? null) === (t.parent ?? null);
      const g = this.ghost;
      const drop = handoff && g.pull ? this.ghostDrawnAt(g.pull) : null;
      const ox = drop ? drop.x : first ? t.x : from ? from.x.v : this.core.x.v;
      const oy = drop ? drop.y : first ? t.y : from ? from.y.v : this.core.y.v;
      if (handoff) {
        this.pending = null;
        snap(g.o, 0);
        g.pull = null;
      }
      this.nodes.set(t.id, {
        id: t.id,
        x: spring(ox, t.x),
        y: spring(oy, t.y),
        s: spring(first || drop ? 1 : 0.25, 1),
        o: spring(first || drop ? 1 : 0, 1),
        labelAt: drop ? performance.now() : null,
        rx: spring(t.rel.x),
        ry: spring(t.rel.y),
        swap: null,
        parent: t.parent,
        lx: spring(0),
        ly: spring(0),
        data: t.data,
        leaving: false,
        fly: null,
        turn: [...t.id].reduce((a, ch) => a + ch.charCodeAt(0), 0) % 2 ? 1 : -1,
      });
    }
    for (const n of this.nodes.values()) {
      if (seen.has(n.id) || n.leaving) continue;
      structural = true;
      const into = foldInto(n.id);
      const holder = into ? this.nodes.get(into) : undefined;
      if (holder) {
        // Folding: back into the clove whose bulb holds it.
        n.leaving = "fold";
        n.x.to = holder.x.to;
        n.y.to = holder.y.to;
        n.s.to = 0.3;
      } else {
        // Gone without a holder: it flies (startFlights set it going).
        continue;
      }
      n.o.to = 0;
    }
    if (structural) this.changed();
    this.kick();
  }

  /** A clove let go mid-pull: it carries on from where it was dropped, on the same springs. */
  absorb(id: string, dx: number, dy: number) {
    for (const m of this.lineage(id)) {
      const n = this.nodes.get(m);
      if (!n) continue;
      n.x.v += dx;
      n.y.v += dy;
    }
    this.kick();
  }

  /** A clove and every clove hanging from it on the map, it first. */
  lineage(id: string): string[] {
    const out = [id];
    for (let i = 0; i < out.length; i++)
      for (const n of this.nodes.values()) if (n.parent === out[i] && !n.leaving && !out.includes(n.id)) out.push(n.id);
    return out;
  }

  /** These cloves are about to be ripped off together: they'll fly off along the pulled one's way out. */
  ripGroup(id: string) {
    const n = this.nodes.get(id);
    if (!n) return;
    const dir = { x: n.x.v - this.core.x.v, y: n.y.v - this.core.y.v };
    for (const m of this.lineage(id)) this.flyDir.set(m, dir);
  }

  /** The label of `id` rises by (dx, dy) world units (its menu is open); null drops it back. */
  setLift(id: string | null, dx = 0, dy = 0) {
    for (const n of this.nodes.values()) {
      const on = n.id === id;
      n.lx.to = on ? dx : 0;
      n.ly.to = on ? dy : 0;
    }
    this.kick();
  }

  /**
   * The pull in progress (or null), drawn on this clock. `short`: it was let
   * go before tearing (or arming), so a copy shrinks back into where it came
   * from; otherwise it fades where it is.
   */
  setPull(p: Pull | null, short = false) {
    const g = this.ghost;
    const was = this.pull;
    this.pull = p;
    // A rip moves the pulled clove's whole lineage with it.
    if (p?.target.kind === "clone" && !p.fork) {
      if (was?.target.kind !== "clone" || was.target.name !== p.target.name || was.fork)
        this.pullGroup = new Set(this.lineage(p.target.name));
    } else this.pullGroup = new Set();
    // Let go past tearing (a sprout or a fork's copy): it waits where it was
    // dropped for its new clone, which then starts from there.
    if (p?.held && ghostly(p) && !this.pending)
      this.pending = { parent: p.target.kind === "clone" ? p.target.name : null, releasedAt: 0 };
    if (ghostly(p) && p) {
      if (!ghostly(was)) {
        this.cancelled = false;
        g.s.v = 0.3;
        g.o.v = 0;
      }
      g.pull = p;
      g.o.to = 1;
      const at = this.ghostAt(p);
      snap(g.x, at.x);
      snap(g.y, at.y);
    } else if (ghostly(was) && was && this.pending) {
      // Its op is done; the new clone is on its way: hold the drop point a moment longer.
      g.pull = was;
      const at = this.ghostAt(was);
      snap(g.x, at.x);
      snap(g.y, at.y);
      snap(g.s, 1);
      this.pending.releasedAt ||= performance.now();
    } else if (ghostly(was) && was) {
      g.pull = was;
      g.o.to = 0;
      const at = this.ghostAt(was);
      snap(g.x, at.x);
      snap(g.y, at.y);
      // A short pull, or a copy whose op failed (cancelled before the pull let go): back home.
      if (short || this.cancelled) {
        this.cancelled = false;
        const home = this.ghostHome(was);
        g.x.to = home.x;
        g.y.to = home.y;
        g.s.to = 0.2;
      }
    }
    this.kick();
  }

  /**
   * A dropped sprout or fork copy waiting for its clone: the next new clove
   * that fits (a top-level clone for a sprout, a fork of `parent` for a copy)
   * starts from the drop point instead of growing from the bulb.
   */
  pending: { parent: string | null; releasedAt: number } | null = null;

  /** Its clone isn't coming (the op failed, or too long): the dropped copy shrinks back home. */
  /** The pending copy was cancelled while its pull is still held: let go, it goes home. */
  cancelled = false;
  cancelPending() {
    const g = this.ghost;
    if (!this.pending) return;
    this.pending = null;
    this.cancelled = this.pull === g.pull;
    g.o.to = 0;
    if (g.pull) {
      const home = this.ghostHome(g.pull);
      g.x.to = home.x;
      g.y.to = home.y;
    }
    g.s.to = 0.2;
    this.kick();
  }

  /** Where a pulled copy (a fork's) or sprout (the image's) is, from its pull. */
  ghostAt(p: Pull) {
    const home = this.ghostHome(p);
    return { x: home.x + p.dx, y: home.y + p.dy };
  }
  /** Where the copy is drawn: a sprout sits at least on the bulb's rim. */
  ghostDrawnAt(p: Pull) {
    const at = this.ghost.pull === this.pull ? this.ghostAt(p) : { x: this.ghost.x.v, y: this.ghost.y.v };
    if (p.target.kind !== "image") return at;
    const dx = at.x - this.core.x.v;
    const dy = at.y - this.core.y.v;
    const l = Math.hypot(dx, dy) || 1;
    const out = Math.max(l, 70);
    return { x: this.core.x.v + (dx / l) * out, y: this.core.y.v + (dy / l) * out };
  }

  /** Where a pull started from: the clove's centre, or the press point on the image. */
  ghostHome(p: Pull) {
    if (p.target.kind === "clone") {
      const n = this.nodes.get(p.target.name);
      if (n) return { x: n.x.v, y: n.y.v };
    }
    return p.from;
  }

  /** The image is refreshing: a new skin forms round the bulb, on this clock. */
  breathing = false;
  /** When the image last moved on a generation (its rims shift in), and when a refresh last failed. */
  shiftAt = -1e9;
  failAt = -1e9;
  /** An empty map invites a pull: the ghost clove's nudge runs while it shows, further when the bulb is hovered. */
  inviting = false;
  bulbHover = false;
  setBreathing(on: boolean) {
    this.breathing = on;
    this.kick();
  }

  setPeeling(names: ReadonlySet<string>) {
    this.peeling = names;
    if (names.size) this.kick();
  }

  /** Where a clove is drawn right now (its pull included), in world units. */
  at(id: string): { x: number; y: number } | undefined {
    const n = this.nodes.get(id);
    if (!n) return undefined;
    const off = this.pullOffset(id);
    return { x: n.x.v + off.dx, y: n.y.v + off.dy };
  }

  /** How far a rip pull moves this clove off its spring: the pulled one and its whole lineage, rigidly. */
  pullOffset(id: string) {
    const p = this.pull;
    if (p && p.target.kind === "clone" && !p.fork && this.pullGroup.has(id)) return { dx: p.dx, dy: p.dy };
    return { dx: 0, dy: 0 };
  }

  // ---- the clock ------------------------------------------------------------

  kick() {
    if (this.raf) return;
    this.last = performance.now();
    this.raf = requestAnimationFrame(this.frame);
  }

  stop() {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
  }

  private frame = (now: number) => {
    this.raf = 0;
    // Big gaps (a hidden tab) don't fling things: at most two 60Hz frames.
    const dt = Math.min(1 / 30, Math.max(0, (now - this.last) / 1000));
    this.last = now;
    let moving = false;
    const sub = 2;
    for (let i = 0; i < sub; i++) {
      const h = dt / sub;
      step(this.core.x, h);
      step(this.core.y, h);
      for (const r of this.rings.values()) step(r, h);
      for (const n of this.nodes.values()) {
        if (n.fly) continue;
        step(n.x, h);
        step(n.y, h);
        step(n.s, h);
        step(n.o, h);
        step(n.rx, h);
        step(n.ry, h);
        step(n.lx, h);
        step(n.ly, h);
      }
      // While pulled, the copy is where the pointer says (ghostAt); its
      // springs take over once it's let go.
      const g = this.ghost;
      step(g.x, h);
      step(g.y, h);
      step(g.s, h);
      step(g.o, h);
    }
    if (this.deferred && now >= this.deferUntil) {
      const args = this.deferred;
      this.deferred = null;
      this.applyTargets(...args);
    }
    // A flying clove keeps a little of its release speed, then drifts out and fades, on a clock of its own length.
    for (const n of this.nodes.values()) {
      const f = n.fly;
      if (!f) continue;
      const t = Math.min(1, (now - f.at) / FLY_MS);
      const d = FLY * (0.22 * t + 0.78 * t * t);
      n.x.v = n.x.to = f.x + f.ux * d;
      n.y.v = n.y.to = f.y + f.uy * d;
      n.o.v = n.o.to = 1 - t ** 1.6;
      n.o.vel = 0;
    }
    const removed: string[] = [];
    for (const n of this.nodes.values()) {
      if (n.fly) {
        if (now - n.fly.at >= FLY_MS) removed.push(n.id);
        else moving = true;
        continue;
      }
      if (n.leaving && n.o.v < 0.02 && Math.abs(n.o.vel) < 0.5) {
        removed.push(n.id);
        continue;
      }
      if (
        !(
          settled(n.x) &&
          settled(n.y) &&
          settled(n.s, 0.005) &&
          settled(n.o, 0.005) &&
          settled(n.lx) &&
          settled(n.ly) &&
          settled(n.rx) &&
          settled(n.ry)
        )
      )
        moving = true;
    }
    if (!(settled(this.core.x) && settled(this.core.y))) moving = true;
    for (const r of this.rings.values()) if (!settled(r)) moving = true;
    const g = this.ghost;
    if (this.pull || !settled(g.o, 0.005) || !settled(g.s, 0.005)) moving = true;
    if (this.peeling.size || this.breathing || this.deferred) moving = true;
    if (now - this.shiftAt < SHIFT_MS || now - this.failAt < FAIL_MS) moving = true;
    if (this.inviting) moving = true;
    if (this.pending?.releasedAt && now - this.pending.releasedAt > 3000) this.cancelPending();
    if (this.pending) moving = true;
    for (const n of this.nodes.values()) if (n.labelAt !== null && now - n.labelAt < LABEL_IN_MS + 400) moving = true;
    for (const n of this.nodes.values()) {
      if (n.swap && now - n.swap.at > SWAP_MS) n.swap = null;
      if (n.swap) moving = true;
    }
    for (const w of this.writers) w(now);
    if (removed.length) {
      for (const id of removed) this.nodes.delete(id);
      this.changed();
    }
    if (moving) this.kick();
  };
}

const ghostly = (q: Pull | null) => !!q && (q.target.kind === "image" || !!q.fork);

/** A label moving further than this fades across instead of gliding. */
const SWAP_PX = 30;
export const SWAP_MS = 280;

/** How far a removed clove flies outward, enough to leave the view at a fitted zoom. */
export const FLY = 900;
