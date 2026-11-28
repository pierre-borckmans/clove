import type { FocusEvent, KeyboardEvent, MouseEvent, PointerEvent } from "react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

type Point = { x: number; y: number };
type Camera = Point & { scale: number };
/** A box in viewport px. */
export type Box = { left: number; top: number; right: number; bottom: number };
type GestureEvent = Event & { scale: number; clientX: number; clientY: number };
const MAX_SCALE = 4;
/** A small map fits up to this zoom, so it fills a big pane instead of floating in it. */
const MAX_FIT = 1.5;

/** One notch of a mouse wheel zooms by this much. */
const NOTCH_ZOOM = 1.1;
/** Zoom per pixel of a smooth (trackpad) delta. */
const SMOOTH_ZOOM = 0.004;

/**
 * The zoom factor for a ⌘/Ctrl wheel event. A mouse wheel's notch (a line
 * or page delta, or a big pixel jump) is a fixed step; a trackpad's stream
 * of small pixel deltas scales smoothly, each event capped so a fling can't jump.
 */
export function wheelZoom(dy: number, mode: number) {
  if (dy === 0) return 1;
  if (mode !== 0 || Math.abs(dy) >= 50) return dy > 0 ? 1 / NOTCH_ZOOM : NOTCH_ZOOM;
  return Math.exp(-Math.max(-25, Math.min(25, dy)) * SMOOTH_ZOOM);
}

/** A local canvas camera: gestures change the map, never the webview's zoom. */
/** `bottomInset` is viewport height kept clear (an overlay at the bottom); fitting centres above it. */
export function useMapCamera(worldWidth: number, worldHeight: number, bottomInset = 0) {
  const viewport = useRef<HTMLElement>(null);
  const [camera, setCamera] = useState<Camera>({ x: 0, y: 0, scale: 1 });
  const liveCamera = useRef(camera);
  const [dragging, setDragging] = useState(false);
  const [hand, setHand] = useState(false);
  const space = useRef(false);
  const pointers = useRef(new Map<number, Point>());
  const drag = useRef<{ point: Point; camera: Camera } | null>(null);
  const pinch = useRef<{ distance: number; midpoint: Point; camera: Camera } | null>(null);
  const nativePinch = useRef<{ point: Point; camera: Camera } | null>(null);
  const suppressClick = useRef(false);
  const world = useRef({ width: worldWidth, height: worldHeight });
  world.current = { width: worldWidth, height: worldHeight };
  const inset = useRef(bottomInset);
  inset.current = bottomInset;
  /** Height of a drawer over the viewport's bottom: the map may pan up that far more, never forced to. */
  const overlay = useRef(0);

  /** The scale that fits the whole map, enlarging a small one up to MAX_FIT. */
  const fitScale = useCallback(() => {
    const el = viewport.current;
    if (!el) return 1;
    const { width, height } = world.current;
    return Math.min(MAX_FIT, (el.clientWidth - 48) / width, (el.clientHeight - 40 - inset.current) / height);
  }, []);
  // Zooming out stops at the fit, or at 100% for a map the fit enlarged.
  const clampScale = useCallback(
    (scale: number) => Math.max(Math.max(0.05, Math.min(1, fitScale())), Math.min(MAX_SCALE, scale)),
    [fitScale],
  );

  // A map smaller than the viewport stays centred; a larger one pans until
  // its edge meets the viewport's. The map carries its own padding.
  /** A camera glide in flight (fit from the button, double-click or key); any gesture stops it. */
  const glide = useRef<number | null>(null);
  /** The camera kept inside the map's bounds (plus room to pan above a drawer). */
  const bound = useCallback((next: Camera): Camera => {
    const el = viewport.current;
    if (!el) return next;
    const axis = (pos: number, view: number, size: number, extra = 0) => {
      const centered = (view - size) / 2;
      const lo = Math.min(view - size - extra, centered - extra);
      const hi = Math.max(0, centered);
      return Math.min(hi, Math.max(lo, pos));
    };
    return {
      scale: next.scale,
      x: axis(next.x, el.clientWidth, world.current.width * next.scale),
      y: axis(next.y, el.clientHeight - inset.current, world.current.height * next.scale, overlay.current),
    };
  }, []);
  const update = useCallback(
    (next: Camera, gliding = false) => {
      if (!gliding && glide.current !== null) {
        cancelAnimationFrame(glide.current);
        glide.current = null;
      }
      // A glide ends on the fit, inside the bounds: clamping every frame on the
      // way would snap the path.
      const bounded = gliding ? next : bound(next);
      liveCamera.current = bounded;
      setCamera(bounded);
    },
    [bound],
  );

  const localPoint = useCallback((clientX: number, clientY: number): Point => {
    const rect = viewport.current?.getBoundingClientRect();
    return { x: clientX - (rect?.left ?? 0), y: clientY - (rect?.top ?? 0) };
  }, []);

  const zoomAt = useCallback(
    (factor: number, point: Point, from = liveCamera.current) => {
      const scale = clampScale(from.scale * factor);
      const ratio = scale / from.scale;
      update({ x: point.x - (point.x - from.x) * ratio, y: point.y - (point.y - from.y) * ratio, scale });
    },
    [update, clampScale],
  );

  const zoomBy = useCallback(
    (factor: number) => {
      zoomAt(factor, { x: (viewport.current?.clientWidth ?? 0) / 2, y: (viewport.current?.clientHeight ?? 0) / 2 });
    },
    [zoomAt],
  );

  const fitTarget = useCallback((): Camera | null => {
    if (!viewport.current) return null;
    const { clientWidth: width, clientHeight: height } = viewport.current;
    const scale = Math.max(0.05, fitScale());
    return { x: (width - worldWidth * scale) / 2, y: (height - bottomInset - worldHeight * scale) / 2, scale };
  }, [fitScale, worldHeight, worldWidth, bottomInset]);

  /** Fits at once: on mount and on resize. */
  const fitNow = useCallback(() => {
    const target = fitTarget();
    if (target) update(target);
  }, [fitTarget, update]);

  /**
   * Glides to the fit (the button, a double-click, 0 or F). Scale moves in log
   * space and the position follows it, so the zoom feels even; ease-out cubic.
   */
  const glideTo = useCallback(
    (target: Camera) => {
      const from = liveCamera.current;
      if (glide.current !== null) cancelAnimationFrame(glide.current);
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return update(target);
      const el = viewport.current;
      if (!el) return;
      // Glide the world point at the viewport's centre, not the camera's
      // corner: with the scale moving in log space, that keeps the path straight.
      const cx = el.clientWidth / 2;
      const cy = (el.clientHeight - inset.current) / 2;
      const centre = (c: Camera) => ({ x: (cx - c.x) / c.scale, y: (cy - c.y) / c.scale });
      const a = centre(from);
      const b = centre(target);
      const start = performance.now();
      const ms = 420;
      const step = (now: number) => {
        const t = Math.min(1, (now - start) / ms);
        const e = 1 - (1 - t) ** 3;
        const lerp = (p: number, q: number) => p + (q - p) * e;
        const scale = Math.exp(lerp(Math.log(from.scale), Math.log(target.scale)));
        const wx = lerp(a.x, b.x);
        const wy = lerp(a.y, b.y);
        update(t < 1 ? { x: cx - wx * scale, y: cy - wy * scale, scale } : target, t < 1);
        glide.current = t < 1 ? requestAnimationFrame(step) : null;
      };
      glide.current = requestAnimationFrame(step);
    },
    [update],
  );

  const fit = useCallback(() => {
    const target = fitTarget();
    if (target) glideTo(target);
  }, [fitTarget, glideTo]);

  /**
   * Glides just enough that `box` (viewport px) sits inside the viewport,
   * `margin` in from its edges and above the bottom inset and any drawer. When
   * it can't fit, its top (and left) edge wins: better a whole menu ring over
   * the drawer than one cut off at the top.
   */
  const keepVisible = useCallback(
    (box: Box, margin = 12) => {
      const el = viewport.current;
      if (!el) return;
      const live = liveCamera.current;
      const fit = (lo: number, hi: number, a: number, b: number) => {
        if (b > hi) return Math.min(b - hi, Math.max(0, a - lo));
        if (a < lo) return a - lo;
        return 0;
      };
      const dx = fit(margin, el.clientWidth - margin, box.left, box.right);
      const dy = fit(margin, el.clientHeight - inset.current - overlay.current - margin, box.top, box.bottom);
      if (Math.abs(dx) > 1 || Math.abs(dy) > 1) glideTo({ ...live, x: live.x - dx, y: live.y - dy });
    },
    [glideTo],
  );

  /**
   * A drawer of `height` px now covers the viewport's bottom: the map may pan
   * up that much more, and `keep` (viewport px) glides above it. With no
   * drawer, glides back inside the map's usual bounds.
   */
  const setDrawer = useCallback(
    (height: number, keep?: Box) => {
      overlay.current = height;
      const live = liveCamera.current;
      if (height && keep) keepVisible(keep);
      else if (!height) {
        const bounded = bound(live);
        if (Math.abs(bounded.y - live.y) > 1) glideTo(bounded);
      }
    },
    [glideTo, bound, keepVisible],
  );

  // Fit before the first paint, so the map never shows a frame at 100% in a
  // corner before the resize observer below catches up.
  // biome-ignore lint/correctness/useExhaustiveDependencies: once, on mount.
  useLayoutEffect(() => {
    fitNow();
  }, []);

  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    let previous = { width: 0, height: 0 };
    const observer = new ResizeObserver(() => {
      const width = element.clientWidth;
      const height = element.clientHeight;
      if (!previous.width) fitNow();
      else {
        const current = liveCamera.current;
        update({
          ...current,
          x: current.x + (width - previous.width) / 2,
          y: current.y + (height - previous.height) / 2,
        });
      }
      previous = { width, height };
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [fitNow, update]);

  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      if (nativePinch.current) return;
      // Trackpad scroll pans. Pinch (Ctrl+wheel in Chromium), or modified
      // mouse wheel zooms about the cursor rather than the canvas origin.
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? element.clientHeight : 1;
      if (event.ctrlKey || event.metaKey) {
        zoomAt(wheelZoom(event.deltaY * unit, event.deltaMode), localPoint(event.clientX, event.clientY));
      } else {
        const current = liveCamera.current;
        update({ ...current, x: current.x - event.deltaX * unit, y: current.y - event.deltaY * unit });
      }
    };
    // WKWebView reports trackpad pinches as WebKit gesture events.
    const gestureStart = (event: Event) => {
      event.preventDefault();
      const gesture = event as GestureEvent;
      const point = localPoint(gesture.clientX, gesture.clientY);
      nativePinch.current = {
        point:
          Number.isFinite(point.x) && Number.isFinite(point.y) && point.x >= 0 && point.y >= 0
            ? point
            : { x: element.clientWidth / 2, y: element.clientHeight / 2 },
        camera: liveCamera.current,
      };
    };
    const gestureChange = (event: Event) => {
      event.preventDefault();
      const start = nativePinch.current;
      if (start) zoomAt((event as GestureEvent).scale, start.point, start.camera);
    };
    const gestureEnd = (event: Event) => {
      event.preventDefault();
      nativePinch.current = null;
    };
    element.addEventListener("wheel", wheel, { passive: false });
    element.addEventListener("gesturestart", gestureStart, { passive: false });
    element.addEventListener("gesturechange", gestureChange, { passive: false });
    element.addEventListener("gestureend", gestureEnd, { passive: false });
    return () => {
      element.removeEventListener("wheel", wheel);
      element.removeEventListener("gesturestart", gestureStart);
      element.removeEventListener("gesturechange", gestureChange);
      element.removeEventListener("gestureend", gestureEnd);
    };
  }, [localPoint, update, zoomAt]);

  const onPointerDown = (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0 && event.button !== 1) return;
    suppressClick.current = false;
    const node = (event.target as Element).closest('button, [role="button"]');
    if (node && !space.current && event.button === 0 && event.pointerType !== "touch") return;
    const point = localPoint(event.clientX, event.clientY);
    pointers.current.set(event.pointerId, point);
    if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()];
      pinch.current = {
        distance: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)),
        midpoint: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 },
        camera: liveCamera.current,
      };
      suppressClick.current = true;
      event.preventDefault();
      for (const id of pointers.current.keys()) event.currentTarget.setPointerCapture(id);
    } else drag.current = { point, camera: liveCamera.current };
    if (!node || space.current || event.button === 1) {
      event.preventDefault();
      event.currentTarget.focus({ preventScroll: true });
      event.currentTarget.setPointerCapture(event.pointerId);
    }
  };

  const onPointerMove = (event: PointerEvent<HTMLElement>) => {
    if (!pointers.current.has(event.pointerId)) return;
    const point = localPoint(event.clientX, event.clientY);
    pointers.current.set(event.pointerId, point);
    const start = pinch.current;
    if (start && pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()];
      const midpoint = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
      const scale = clampScale((start.camera.scale * Math.hypot(a.x - b.x, a.y - b.y)) / start.distance);
      const ratio = scale / start.camera.scale;
      update({
        x: midpoint.x - (start.midpoint.x - start.camera.x) * ratio,
        y: midpoint.y - (start.midpoint.y - start.camera.y) * ratio,
        scale,
      });
    } else if (drag.current) {
      const dx = point.x - drag.current.point.x;
      const dy = point.y - drag.current.point.y;
      if (!suppressClick.current && Math.hypot(dx, dy) < 4) return;
      update({ ...drag.current.camera, x: drag.current.camera.x + dx, y: drag.current.camera.y + dy });
    } else return;
    event.preventDefault();
    suppressClick.current = true;
    setDragging(true);
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onPointerUp = (event: PointerEvent<HTMLElement>) => {
    pointers.current.delete(event.pointerId);
    pinch.current = null;
    const remaining = [...pointers.current.values()][0];
    drag.current = remaining ? { point: remaining, camera: liveCamera.current } : null;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    if (!remaining) setDragging(false);
  };

  const onPointerCancel = () => {
    pointers.current.clear();
    drag.current = null;
    pinch.current = null;
    setDragging(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.code === "Space" && event.target === event.currentTarget && !event.metaKey && !event.ctrlKey) {
      space.current = true;
      setHand(true);
      event.preventDefault();
      return;
    }
    if (event.target !== event.currentTarget || event.metaKey || event.ctrlKey || event.altKey) return;
    const current = liveCamera.current;
    if (event.key === "+" || event.key === "=") zoomBy(1.2);
    else if (event.key === "-") zoomBy(1 / 1.2);
    else if (event.key === "0" || event.key.toLowerCase() === "f") fit();
    else if (event.key === "ArrowLeft") update({ ...current, x: current.x + 40 });
    else if (event.key === "ArrowRight") update({ ...current, x: current.x - 40 });
    else if (event.key === "ArrowUp") update({ ...current, y: current.y + 40 });
    else if (event.key === "ArrowDown") update({ ...current, y: current.y - 40 });
    else return;
    event.preventDefault();
  };

  return {
    viewport,
    camera,
    dragging,
    hand,
    fit,
    zoomBy,
    setDrawer,
    keepVisible,
    bindings: {
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel,
      onLostPointerCapture: (event: PointerEvent<HTMLElement>) => {
        if (pointers.current.has(event.pointerId)) onPointerUp(event);
      },
      onClickCapture: (event: MouseEvent<HTMLElement>) => {
        if (event.detail > 0 && suppressClick.current) {
          event.preventDefault();
          event.stopPropagation();
          suppressClick.current = false;
        }
      },
      // On empty canvas; a clone's own double-click opens it.
      onDoubleClick: (event: MouseEvent<HTMLElement>) => {
        if (!(event.target as Element).closest('button, [role="button"]')) fit();
      },
      onFocusCapture: (event: FocusEvent<HTMLElement>) => {
        const node = (event.target as Element).closest('button, [role="button"]');
        if (!node || !viewport.current) return;
        const bounds = viewport.current.getBoundingClientRect();
        const box = node.getBoundingClientRect();
        const dx =
          box.left < bounds.left + 20
            ? bounds.left + 20 - box.left
            : box.right > bounds.right - 20
              ? bounds.right - 20 - box.right
              : 0;
        const dy =
          box.top < bounds.top + 20
            ? bounds.top + 20 - box.top
            : box.bottom > bounds.bottom - 20
              ? bounds.bottom - 20 - box.bottom
              : 0;
        if (dx || dy) update({ ...liveCamera.current, x: liveCamera.current.x + dx, y: liveCamera.current.y + dy });
      },
      onKeyDown,
      onKeyUp: (event: KeyboardEvent<HTMLElement>) => {
        if (event.code === "Space") {
          space.current = false;
          setHand(false);
        }
      },
      onBlur: () => {
        space.current = false;
        setHand(false);
      },
    },
  };
}
