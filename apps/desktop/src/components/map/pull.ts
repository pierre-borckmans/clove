import { type MouseEvent, type PointerEvent, useCallback, useRef, useState } from "react";
import type { MenuTarget } from "./nodes";

/** Screen pixels a pull travels before the clove tears free. */
export const TEAR_PX = 130;
/** A torn clove mends if brought back this close. */
const MEND_PX = TEAR_PX * 0.6;
/** How long a clove springs back home. */
export const SPRING_MS = 420;

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
};

/** Before it tears, a clove only gives a little, like it's still attached. */
const give = (d: number) => TEAR_PX * 0.42 * (1 - Math.exp(-d / (TEAR_PX * 0.55)));

/**
 * Press and drag a clone or the image. A short drag springs back; past
 * {@link TEAR_PX} the clove tears free and follows the pointer, and letting go
 * calls `onTear`. The caller releases a held pull once its dialog is done.
 */
export function usePull({
  scale,
  toWorld,
  onTear,
}: {
  scale: number;
  toWorld: (x: number, y: number) => { x: number; y: number } | undefined;
  onTear: (pull: Pull) => void;
}) {
  const [pull, setPull] = useState<Pull | null>(null);
  /** What just sprang back, for its transition. */
  const [returning, setReturning] = useState<string | null>(null);
  const dragged = useRef(false);
  const live = useRef({ scale, toWorld, onTear });
  live.current = { scale, toWorld, onTear };
  const held = !!pull?.held;

  const start = (target: MenuTarget, event: PointerEvent<HTMLElement>) => {
    // Touch pans the map; only a mouse or pen pulls.
    if (event.button !== 0 || event.pointerType === "touch" || held) return;
    const { clientX: x0, clientY: y0, pointerId: id } = event;
    const from = live.current.toWorld(x0, y0);
    if (!from) return;
    dragged.current = false;
    let torn = false;
    let last: Pull | null = null;
    const move = (e: globalThis.PointerEvent) => {
      if (e.pointerId !== id) return;
      const sx = e.clientX - x0;
      const sy = e.clientY - y0;
      const d = Math.hypot(sx, sy);
      if (!dragged.current && d < 5) return;
      dragged.current = true;
      torn = torn ? d > MEND_PX : d > TEAR_PX;
      const k = torn ? 1 : give(d) / d;
      const s = live.current.scale;
      last = { target, from, dx: (sx * k) / s, dy: (sy * k) / s, strain: torn ? 1 : Math.min(1, d / TEAR_PX), torn };
      setPull(last);
    };
    const end = (e: globalThis.PointerEvent) => {
      if (e.pointerId !== id) return;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      if (!last) return;
      if (last.torn && e.type === "pointerup") {
        // A clove out of the image waits where it was dropped for its clone;
        // one ripped off a clone is the caller's to fly off, so the next pull
        // can start at once.
        if (target.kind === "image") setPull({ ...last, held: true });
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

  const springBack = useCallback((p: Pull | null) => {
    if (!p) return;
    const key = p.target.kind === "clone" ? p.target.name : "image";
    setReturning(key);
    setPull(null);
    window.setTimeout(() => setReturning((r) => (r === key ? null : r)), SPRING_MS);
  }, []);

  /** The click that ends a drag would open the menu: swallow it. */
  const swallowClick = (event: MouseEvent) => {
    if (!dragged.current) return;
    dragged.current = false;
    event.stopPropagation();
    event.preventDefault();
  };

  return { pull, returning, start, swallowClick, springBack, setPull };
}
