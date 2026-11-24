import { useSyncExternalStore } from "react";

export type Theme = "light" | "dark";

const params = new URLSearchParams(window.location.search);
const param = params.get("theme");
const forced: Theme | null = param === "dark" || param === "light" ? param : null;
const media = window.matchMedia("(prefers-color-scheme: dark)");

function current(): Theme {
  return forced ?? (media.matches ? "dark" : "light");
}

function apply() {
  document.documentElement.classList.toggle("dark", current() === "dark");
}

/** Follows the system appearance; `?theme=dark` pins it (screenshots). */
export function setupTheme() {
  apply();
  media.addEventListener("change", apply);
}

export function useTheme(): Theme {
  return useSyncExternalStore((cb) => {
    media.addEventListener("change", cb);
    return () => media.removeEventListener("change", cb);
  }, current);
}
