// Which global dialogs are open. Kept outside React so keyboard shortcuts,
// the command palette and backend events can all open them.

import { useSyncExternalStore } from "react";
import type { CloneInfo } from "./types";

export type UiState = {
  palette: boolean;
  newClone: { root: string | null; branch?: string } | null;
  addRepo: boolean;
  remove: { root: string; clone: CloneInfo } | null;
  activity: boolean;
};

let state: UiState = { palette: false, newClone: null, addRepo: false, remove: null, activity: false };
const subs = new Set<() => void>();

export function setUi(p: Partial<UiState>) {
  state = { ...state, ...p };
  for (const s of subs) s();
}

export function getUi() {
  return state;
}

export function useUi(): UiState {
  return useSyncExternalStore(
    (cb) => {
      subs.add(cb);
      return () => subs.delete(cb);
    },
    () => state,
  );
}
