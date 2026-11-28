// Which global dialogs are open. Kept outside React so keyboard shortcuts,
// the command palette and backend events can all open them.

import { useSyncExternalStore } from "react";
import type { CloneInfo, OtherWorktree } from "./types";

export type UiState = {
  palette: boolean;
  newClone: { root: string | null; branch?: string } | null;
  addRepo: boolean;
  /**
   * Remove one clone, or several picked in the list. `forks`: the clone's
   * forks (deepest first), offered as "Also remove N forks".
   */
  remove: { root: string; clones: CloneInfo[]; forks?: CloneInfo[] } | null;
  /** Remove plain git worktrees (one, or several picked). */
  removeWorktree: { root: string; worktrees: OtherWorktree[] } | null;
  /** Turn a plain git worktree into a clone. */
  adopt: { root: string; worktree: OtherWorktree } | null;
  /**
   * Per repo root, lineages opened or closed by hand, in the map (bulbs) and
   * the list (tree rows) alike. Not saved. Unset, a big lineage starts folded
   * and a small one open (map/lineage.ts foldable).
   */
  openBulbs: Record<string, readonly string[]>;
  closedBulbs: Record<string, readonly string[]>;
  /** Pick a checkpoint to rewind a clone to. */
  rewind: { root: string; clone: CloneInfo } | null;
  activity: boolean;
  /** The update dialog. */
  upgrade: boolean;
};

/** Shows (or, with `open` false, folds) these clones' forks, on the map and in the list of `root`. */
export function setBulbs(root: string, names: string[], open: boolean) {
  const opened = new Set(state.openBulbs[root] ?? []);
  const closed = new Set(state.closedBulbs[root] ?? []);
  for (const n of names) {
    if (open) {
      opened.add(n);
      closed.delete(n);
    } else {
      opened.delete(n);
      closed.add(n);
    }
  }
  setUi({
    openBulbs: { ...state.openBulbs, [root]: [...opened] },
    closedBulbs: { ...state.closedBulbs, [root]: [...closed] },
  });
}

let state: UiState = {
  palette: false,
  newClone: null,
  addRepo: false,
  remove: null,
  removeWorktree: null,
  adopt: null,
  openBulbs: {},
  closedBulbs: {},
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
