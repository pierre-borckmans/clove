import { useLayoutEffect, useRef, useState } from "react";

export type MotionItem<T> = {
  id: string;
  /** Where it should be: any list of numbers (positions, radii). */
  values: number[];
  /** Where it comes from when new, and goes back to when removed. */
  origin: number[];
  data: T;
};
export type Moving<T> = MotionItem<T> & {
  /** 0 to 1: how far in it is. Below 1 while entering or leaving. */
  presence: number;
  leaving: boolean;
};

const DURATION = 520;
const ease = (t: number) => 1 - (1 - t) ** 3;
const reducedMotion = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * Tweens items between layouts: moved ones glide, new ones grow out of their
 * origin, removed ones go back into it and fade. The first layout shows as is.
 */
export function useMotion<T>(items: MotionItem<T>[]): Moving<T>[] {
  const shown = useRef<Map<string, Moving<T>> | null>(null);
  const [, setFrame] = useState(0);

  useLayoutEffect(() => {
    const target = new Map(items.map((item) => [item.id, item]));
    const previous = shown.current;
    const settle = () => {
      shown.current = new Map(items.map((item) => [item.id, { ...item, presence: 1, leaving: false }]));
      setFrame((n) => n + 1);
    };
    if (!previous || reducedMotion()) return settle();

    // Each track runs from what is on screen now to its new target.
    const tracks = new Map<string, { from: number[]; to: number[]; p0: number; p1: number; item: MotionItem<T> }>();
    for (const item of items) {
      const now = previous.get(item.id);
      tracks.set(item.id, {
        from: now && !now.leaving ? now.values : item.origin,
        to: item.values,
        p0: now && !now.leaving ? now.presence : 0,
        p1: 1,
        item,
      });
    }
    for (const [id, now] of previous) {
      if (target.has(id) || (now.leaving && now.presence === 0)) continue;
      tracks.set(id, { from: now.values, to: now.origin, p0: now.presence, p1: 0, item: now });
    }

    let raf = 0;
    const start = performance.now();
    const step = (time: number) => {
      const t = ease(Math.min(1, (time - start) / DURATION));
      const next = new Map<string, Moving<T>>();
      for (const [id, { from, to, p0, p1, item }] of tracks) {
        const values = to.map((v, i) => (from[i] ?? v) + (v - (from[i] ?? v)) * t);
        const presence = p0 + (p1 - p0) * t;
        if (p1 === 0 && t === 1) continue;
        next.set(id, { ...item, values, presence, leaving: p1 === 0 });
      }
      shown.current = next;
      setFrame((n) => n + 1);
      if (t < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [items]);

  // Before the first effect runs, show the layout as is.
  return shown.current ? [...shown.current.values()] : items.map((item) => ({ ...item, presence: 1, leaving: false }));
}
