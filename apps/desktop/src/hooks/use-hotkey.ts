import { useEffect, useRef } from "react";

export type Hotkey = {
  key: string;
  meta?: boolean;
  shift?: boolean;
  /** Fire even while typing in an input. Defaults to true for ⌘ combos. */
  inInputs?: boolean;
};

function typing(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
}

export function matches(e: KeyboardEvent, h: Hotkey) {
  if (e.key.toLowerCase() !== h.key.toLowerCase()) return false;
  if (!!h.meta !== (e.metaKey || e.ctrlKey)) return false;
  if (h.shift !== undefined && h.shift !== e.shiftKey) return false;
  if (e.altKey) return false;
  if (!(h.inInputs ?? !!h.meta) && typing(e)) return false;
  return true;
}

/** Window-level shortcut. The handler may change between renders. */
export function useHotkey(h: Hotkey, handler: (e: KeyboardEvent) => void, enabled = true) {
  const ref = useRef(handler);
  ref.current = handler;
  const { key, meta, shift, inInputs } = h;
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (matches(e, { key, meta, shift, inInputs })) {
        e.preventDefault();
        ref.current(e);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [key, meta, shift, inInputs, enabled]);
}
