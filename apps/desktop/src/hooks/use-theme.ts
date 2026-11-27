import { useSyncExternalStore } from "react";
import { isTauri } from "@/lib/bridge";
import { getPrefs, setPrefs, subscribePrefs } from "@/lib/prefs";

export type Theme = "light" | "dark";
export type Appearance = "system" | Theme;

const params = new URLSearchParams(window.location.search);
const param = params.get("theme");
/** `?theme=dark` pins it, for screenshots. */
const forced: Theme | null = param === "dark" || param === "light" ? param : null;
const media = window.matchMedia("(prefers-color-scheme: dark)");

function current(): Theme {
  if (forced) return forced;
  const a = getPrefs().appearance;
  if (a !== "system") return a;
  return media.matches ? "dark" : "light";
}

let applied: Appearance | undefined;

function apply() {
  document.documentElement.classList.toggle("dark", current() === "dark");
  // Traffic lights, scrollbars and native menus follow the window's theme;
  // null hands it back to the system.
  const a = getPrefs().appearance;
  if (isTauri && a !== applied) {
    applied = a;
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
      getCurrentWindow()
        .setTheme(a === "system" ? null : a)
        .catch(() => undefined),
    );
  }
}

/** Applies the appearance now and whenever it or the system's changes. */
export function setupTheme() {
  apply();
  media.addEventListener("change", apply);
  subscribePrefs(apply);
}

/** The theme in effect: the pinned one, or the system's. */
export function useTheme(): Theme {
  return useSyncExternalStore((cb) => {
    media.addEventListener("change", cb);
    const un = subscribePrefs(cb);
    return () => {
      media.removeEventListener("change", cb);
      un();
    };
  }, current);
}

const ORDER: Appearance[] = ["system", "light", "dark"];

/** System, light, dark, round again. */
export function cycleAppearance() {
  const a = getPrefs().appearance;
  setPrefs({ appearance: ORDER[(ORDER.indexOf(a) + 1) % ORDER.length] });
}
