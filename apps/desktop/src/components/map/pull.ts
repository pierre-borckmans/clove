import { type MouseEvent, type PointerEvent, useCallback, useRef, useState } from "react";
import type { MenuTarget } from "./nodes";

/** Screen pixels of radius (from the image) a rip gains before the clove tears free. */
export const TEAR_PX = 100;
/**
 * Before tearing, a ripped clove tracks this share of the radius gained:
 * RIP_GIVE at the start, easing to RIP_GIVE_AT_TEAR by the tear, so the
 * jump when it tears is small.
 */
export const RIP_GIVE = 0.75;
export const RIP_GIVE_AT_TEAR = 0.6;
/** Screen pixels of radius a fork's copy gains before letting go forks. */
export const ARM_PX = 24;
/** Back inside this, a pull leaves the release zone (a little below TEAR_PX, so it doesn't flicker). */
const MEND_PX = TEAR_PX * 0.9;
/** Movement that gains no radius gives this share, at most SIDE_CAP pixels. */
const SIDE_GIVE = 0.25;
const SIDE_CAP = 22;

/**
 * A clove pulled off a clone (to remove it) or out of the golden image (to
 * make one). Offsets are in world units, from where the press started.
 */
export type Pull = {
  target: MenuTarget;
  /** Where the press started, in world units. */
  from: { x: number; y: number };
  dx: number;
  dy: number;
  /** How far toward tearing, 0 to 1 (1 once torn). */
  strain: number;
  /** Past the tearing point: letting go removes the clone, or makes one. */
  torn: boolean;
  /** Out of the image and let go: it stays where it was dropped until its clone exists. */
  held?: boolean;
  /**
   * Option/Alt held on a clone when the press started: a copy follows the
   * pointer 1:1 and the clone stays. `torn` means armed (past ARM_PX):
   * letting go forks; before that the copy shrinks back.
   */
  fork?: boolean;
};

/** Before it tears, a clove only gives a little, like it's still attached. */
const give = (d: number) => d * (RIP_GIVE + (RIP_GIVE_AT_TEAR - RIP_GIVE) * Math.min(1, d / TEAR_PX));

/**
 * Press and drag a clone or the image. A short drag springs back; past
 * {@link TEAR_PX} the clove tears free and follows the pointer, and letting go
 * calls `onTear`. The caller releases a held pull once its dialog is done.
 */
export function usePull({
  scale,
  toWorld,
  onTear,
  onRelease,
}: {
  scale: number;
  toWorld: (x: number, y: number) => { x: number; y: number } | undefined;
  onTear: (pull: Pull) => void;
  /** Let go: `short` before tearing or arming (it goes back), else torn. Called before the pull clears. */
  onRelease: (pull: Pull, short: boolean) => void;
}) {
  const [pull, setPull] = useState<Pull | null>(null);
  const dragged = useRef(false);
  const live = useRef({ scale, toWorld, onTear, onRelease });
  live.current = { scale, toWorld, onTear, onRelease };
  const held = !!pull?.held;

  /**
   * `centre`: the image's centre on screen. Outward means a bigger radius
   * from it, whatever the direction: a rip only gives and tears, and a fork's
   * copy only arms, by the radius it gains (forks of forks too).
   */
  const start = (target: MenuTarget, event: PointerEvent<Element>, centre?: { x: number; y: number }) => {
    // Touch pans the map; only a mouse or pen pulls.
    if (event.button !== 0 || event.pointerType === "touch" || held) return;
    const { clientX: x0, clientY: y0, pointerId: id } = event;
    const from = live.current.toWorld(x0, y0);
    if (!from) return;
    dragged.current = false;
    /** Screen pixels further from the image's centre than the press was. */
    const gain = (x: number, y: number) =>
      centre
        ? Math.hypot(x - centre.x, y - centre.y) - Math.hypot(x0 - centre.x, y0 - centre.y)
        : Math.hypot(x - x0, y - y0);
    // Decided at the press: a copy has nothing holding it back, a rip does.
    const fork = target.kind === "clone" && event.altKey;
    let torn = false;
    let last: Pull | null = null;
    const move = (e: globalThis.PointerEvent) => {
      if (e.pointerId !== id) return;
      const sx = e.clientX - x0;
      const sy = e.clientY - y0;
      const d = Math.hypot(sx, sy);
      if (!dragged.current && d < 5) return;
      dragged.current = true;
      const s = live.current.scale;
      if (fork) {
        // The copy follows freely, but only arms by radius gained from the image;
        // sideways or inward it stays small and pale, and doesn't fork.
        const ahead = gain(e.clientX, e.clientY);
        torn = ahead > ARM_PX;
        last = { target, from, dx: sx / s, dy: sy / s, strain: Math.max(0, Math.min(1, ahead / ARM_PX)), torn, fork };
        setPull(last);
        return;
      }
      // Only radius gained from the image pulls (strain, tear); the clove
      // still leans toward the pointer, by the rubber band's give for that
      // gain plus a small, capped share of any sideways or inward movement.
      const out = Math.max(0, gain(e.clientX, e.clientY));
      torn = torn ? out > MEND_PX : out > TEAR_PX;
      let mx: number;
      let my: number;
      if (torn) {
        // Torn free: it follows the pointer.
        mx = sx;
        my = sy;
      } else {
        const mag = Math.min(d, give(out) + Math.min(SIDE_CAP, Math.max(0, d - out) * SIDE_GIVE));
        mx = (sx / d) * mag;
        my = (sy / d) * mag;
      }
      last = {
        target,
        from,
        dx: mx / s,
        dy: my / s,
        strain: torn ? 1 : Math.min(1, out / TEAR_PX),
        torn,
      };
      setPull(last);
    };
    const end = (e: globalThis.PointerEvent) => {
      if (e.pointerId !== id) return;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      if (!last) return;
      if (last.torn && e.type === "pointerup") {
        live.current.onRelease(last, false);
        // A clove out of the image, or a fork's copy, waits where it was
        // dropped for its clone; one ripped off a clone is the caller's to
        // fly off, so the next pull can start at once.
        if (target.kind === "image" || last.fork) setPull({ ...last, held: true });
        else setPull(null);
        live.current.onTear(last);
      } else {
        springBack(last);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
  };

  /** Let go short: the scene takes it home from where it is. */
  const springBack = useCallback((p: Pull | null) => {
    if (!p) return;
    live.current.onRelease(p, true);
    setPull(null);
  }, []);

  /** The click that ends a drag would open the menu: swallow it. */
  const swallowClick = (event: MouseEvent) => {
    if (!dragged.current) return;
    dragged.current = false;
    event.stopPropagation();
    event.preventDefault();
  };

  return { pull, start, swallowClick, springBack, setPull };
}
