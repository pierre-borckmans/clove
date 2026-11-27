import { useLayoutEffect, useRef, useState } from "react";

export type MotionItem<T> = {
  id: string;
  /** Where it should be: any list of numbers (positions, radii). */
  values: number[];
  /** Where it comes from when new. */
  origin: number[];
  /** Where it goes when removed; back to `origin` when not given. */
  exit?: number[];
  data: T;
};
export type Moving<T> = MotionItem<T> & {
  /** 0 to 1: how far in it is. Below 1 while entering or leaving. */
  presence: number;
  leaving: boolean;
};

const DURATION = 520;
/** Exits take longer and speed up as they go, like something flung away. */
const LEAVE_DURATION = 900;
const ease = (t: number) => 1 - (1 - t) ** 3;
const easeIn = (t: number) => t ** 2.2;
const reducedMotion = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

type Track<T> = {
  item: MotionItem<T>;
  from: number[];
  to: number[];
  p0: number;
  p1: number;
  /** Each track keeps its own clock, so new data mid-flight doesn't restart it. */
  start: number;
};

const same = (a: number[], b: number[]) => a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) < 0.01);

/** Where a track is at `time`. */
function sample<T>(track: Track<T>, time: number): Moving<T> & { done: boolean } {
  const leaving = track.p1 === 0;
  const raw = Math.min(1, (time - track.start) / (leaving ? LEAVE_DURATION : DURATION));
  const t = leaving ? easeIn(raw) : ease(raw);
  const values = track.to.map((v, i) => (track.from[i] ?? v) + (v - (track.from[i] ?? v)) * t);
  return { ...track.item, values, presence: track.p0 + (track.p1 - track.p0) * t, leaving, done: raw === 1 };
}

/**
 * Tweens items between layouts: moved ones glide, new ones grow out of their
 * origin, removed ones head for their exit and fade. The first layout shows as is.
 */
export function useMotion<T>(items: MotionItem<T>[]): Moving<T>[] {
  const tracks = useRef<Map<string, Track<T>> | null>(null);
  const raf = useRef(0);
  const [, setFrame] = useState(0);

  useLayoutEffect(() => {
    const now = performance.now();
    const previous = tracks.current;
    const next = new Map<string, Track<T>>();
    const still = (item: MotionItem<T>): Track<T> => ({
      item,
      from: item.values,
      to: item.values,
      p0: 1,
      p1: 1,
      start: now - DURATION,
    });

    if (!previous || reducedMotion()) {
      for (const item of items) next.set(item.id, still(item));
      tracks.current = next;
      setFrame((n) => n + 1);
      return;
    }

    for (const item of items) {
      const track = previous.get(item.id);
      if (track && track.p1 === 1 && same(track.to, item.values)) {
        // Same destination: let it run on, with fresh data.
        next.set(item.id, { ...track, item });
        continue;
      }
      const at = track && sample(track, now);
      next.set(item.id, {
        item,
        from: at && !at.leaving ? at.values : item.origin,
        to: item.values,
        p0: at && !at.leaving ? at.presence : 0,
        p1: 1,
        start: now,
      });
    }
    for (const [id, track] of previous) {
      if (next.has(id)) continue;
      if (track.p1 === 0) {
        // Already leaving: keep its flight as it was.
        if (!sample(track, now).done) next.set(id, track);
        continue;
      }
      const at = sample(track, now);
      next.set(id, {
        item: track.item,
        from: at.values,
        to: track.item.exit ?? track.item.origin,
        p0: at.presence,
        p1: 0,
        start: now,
      });
    }
    tracks.current = next;

    cancelAnimationFrame(raf.current);
    const step = (time: number) => {
      const live = tracks.current;
      if (!live) return;
      let running = false;
      for (const [id, track] of live) {
        const at = sample(track, time);
        if (at.done && at.leaving) live.delete(id);
        else if (!at.done) running = true;
      }
      setFrame((n) => n + 1);
      if (running) raf.current = requestAnimationFrame(step);
    };
    raf.current = requestAnimationFrame(step);
  }, [items]);

  useLayoutEffect(() => () => cancelAnimationFrame(raf.current), []);

  const time = performance.now();
  return tracks.current
    ? [...tracks.current.values()].map((track) => sample(track, time))
    : items.map((item) => ({ ...item, presence: 1, leaving: false }));
}
