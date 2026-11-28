// Which global dialogs are open. Kept outside React so keyboard shortcuts,
// the command palette and backend events can all open them.

import { useSyncExternalStore } from "react";
import type { CloneInfo, OtherWorktree } from "./types";

export type UiState = {
  palette: boolean;
  newClone: { root: string | null; branch?: string } | null;
  addRepo: boolean;
  /** Remove one clone, or several picked in the list. */
  remove: { root: string; clones: CloneInfo[] } | null;
  /** Remove plain git worktrees (one, or several picked). */
  removeWorktree: { root: string; worktrees: OtherWorktree[] } | null;
  /** Turn a plain git worktree into a clone. */
  adopt: { root: string; worktree: OtherWorktree } | null;
  /** Pick a checkpoint to rewind a clone to. */
  rewind: { root: string; clone: CloneInfo } | null;
  activity: boolean;
  /** The update dialog. */
  upgrade: boolean;
};

let state: UiState = {
  palette: false,
  newClone: null,
  addRepo: false,
  remove: null,
  removeWorktree: null,
  adopt: null,
  rewind: null,
  activity: false,
  upgrade: false,
};
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
