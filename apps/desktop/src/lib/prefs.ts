import { useSyncExternalStore } from "react";
import { FILE_MANAGER } from "./platform";
import type { AppId } from "./types";

export type Prefs = {
  editor: AppId;
  terminal: AppId;
  diffStyle: "split" | "unified";
  diffWrap: boolean;
  diffSort: "path" | "changes";
  diffGroup: boolean;
  /** Diff view file tree width, px. */
  diffTree: number;
  sidebarCollapsed: boolean;
  /** System follows macOS/Linux appearance; light and dark pin it. */
  appearance: "system" | "light" | "dark";
};

const KEY = "clove.prefs";
const DEFAULTS: Prefs = {
  editor: "vscode",
  terminal: "terminal",
  diffStyle: "split",
  diffWrap: false,
  diffSort: "path",
  diffGroup: false,
  diffTree: 256,
  sidebarCollapsed: false,
  appearance: "system",
};
const subs = new Set<() => void>();

function read(): Prefs {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return DEFAULTS;
  }
}

let current = read();

export function setPrefs(p: Partial<Prefs>) {
  current = { ...current, ...p };
  try {
    localStorage.setItem(KEY, JSON.stringify(current));
  } catch {
    // Storage can be unavailable; the choice then lasts for this session.
  }
  for (const s of subs) s();
}

/** Calls `cb` on every change, including ones made in the app's other window. */
export function subscribePrefs(cb: () => void): () => void {
  subs.add(cb);
  return () => subs.delete(cb);
}

// The main window and the menu-bar popover share storage: keep them in step.
if (typeof window !== "undefined") {
  window.addEventListener("storage", (e) => {
    if (e.key !== KEY) return;
    current = read();
    for (const s of subs) s();
  });
}

export function usePrefs(): Prefs {
  return useSyncExternalStore(
    (cb) => {
      subs.add(cb);
      return () => subs.delete(cb);
    },
    () => current,
  );
}

export function getPrefs(): Prefs {
  return current;
}

export const APP_LABELS: Record<AppId, string> = {
  terminal: "Terminal",
  ghostty: "Ghostty",
  iterm: "iTerm",
  warp: "Warp",
  kitty: "kitty",
  alacritty: "Alacritty",
  wezterm: "WezTerm",
  vscode: "VS Code",
  cursor: "Cursor",
  zed: "Zed",
  finder: FILE_MANAGER,
};

export const EDITORS: AppId[] = ["vscode", "cursor", "zed"];
export const TERMINALS: AppId[] = ["terminal", "ghostty", "iterm", "warp", "kitty", "alacritty", "wezterm"];
