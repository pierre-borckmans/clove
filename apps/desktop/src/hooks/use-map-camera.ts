import type { FocusEvent, KeyboardEvent, MouseEvent, PointerEvent } from "react";
import { useCallback, useEffect, useRef, useState } from "react";

type Point = { x: number; y: number };
type Camera = Point & { scale: number };
type GestureEvent = Event & { scale: number; clientX: number; clientY: number };
const MAX_SCALE = 4;

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

  /** The scale that fits the whole map; zooming out stops there. */
  const fitScale = useCallback(() => {
    const el = viewport.current;
    if (!el) return 1;
    const { width, height } = world.current;
    return Math.min(1, (el.clientWidth - 48) / width, (el.clientHeight - 40 - inset.current) / height);
  }, []);
  const clampScale = useCallback(
    (scale: number) => Math.max(Math.max(0.05, fitScale()), Math.min(MAX_SCALE, scale)),
    [fitScale],
  );

  // A map smaller than the viewport stays centred; a larger one pans until
  // its edge meets the viewport's. The map carries its own padding.
  const update = useCallback((next: Camera) => {
    const el = viewport.current;
    let bounded = next;
    if (el) {
      const axis = (pos: number, view: number, size: number) => {
        const centered = (view - size) / 2;
        const lo = Math.min(view - size, centered);
        const hi = Math.max(0, centered);
        return Math.min(hi, Math.max(lo, pos));
      };
      bounded = {
        scale: next.scale,
        x: axis(next.x, el.clientWidth, world.current.width * next.scale),
        y: axis(next.y, el.clientHeight - inset.current, world.current.height * next.scale),
      };
    }
    liveCamera.current = bounded;
    setCamera(bounded);
  }, []);

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

  const fit = useCallback(() => {
    if (!viewport.current) return;
    const { clientWidth: width, clientHeight: height } = viewport.current;
    const scale = Math.max(0.05, fitScale());
    update({ x: (width - worldWidth * scale) / 2, y: (height - bottomInset - worldHeight * scale) / 2, scale });
  }, [update, fitScale, worldHeight, worldWidth, bottomInset]);

  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    let previous = { width: 0, height: 0 };
    const observer = new ResizeObserver(() => {
      const width = element.clientWidth;
      const height = element.clientHeight;
      if (!previous.width) fit();
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
  }, [fit, update]);

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
        zoomAt(
          Math.exp(-Math.max(-120, Math.min(120, event.deltaY * unit)) * 0.01),
          localPoint(event.clientX, event.clientY),
        );
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
    const node = (event.target as Element).closest("button");
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
        if (!(event.target as Element).closest("button")) fit();
      },
      onFocusCapture: (event: FocusEvent<HTMLElement>) => {
        const node = (event.target as Element).closest("button");
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
