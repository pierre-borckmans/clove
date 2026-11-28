import type { CloneInfo } from "@/lib/types";

/** A lineage folds once it has more forks than this... */
export const BULB_MAX_FORKS = 3;
/** ...or goes deeper than this many forks of forks. */
export const BULB_MAX_DEPTH = 2;

/** Forks of forks, as a forest: a clone whose parent isn't on the map is a root. */
export function forest(clones: CloneInfo[]) {
  const byName = new Map(clones.map((c) => [c.name, c]));
  const parentOf = new Map<string, string>();
  for (const c of clones) {
    const p = c.forked_from;
    if (p && p !== c.name && byName.has(p)) parentOf.set(c.name, p);
  }
  // A loop of names (they get reused) breaks at the clone that closes it.
  for (const c of clones) {
    const seen = new Set<string>();
    for (let up = parentOf.get(c.name); up; up = parentOf.get(up)) {
      if (up === c.name) {
        parentOf.delete(c.name);
        break;
      }
      if (seen.has(up)) break;
      seen.add(up);
    }
  }
  const kids = new Map<string, CloneInfo[]>();
  for (const c of clones) {
    const p = parentOf.get(c.name);
    if (p) kids.set(p, [...(kids.get(p) ?? []), c]);
  }
  for (const list of kids.values()) list.sort((a, b) => a.created_at - b.created_at);
  const descendants = (name: string): number => (kids.get(name) ?? []).reduce((n, k) => n + 1 + descendants(k.name), 0);
  /** Forks of forks below it: 0 for none, 1 for forks only, ... */
  const height = (name: string): number => Math.max(0, ...(kids.get(name) ?? []).map((k) => 1 + height(k.name)));
  return { parentOf, kids, descendants, height, isRoot: (c: CloneInfo) => !parentOf.has(c.name) };
}
export type Forest = ReturnType<typeof forest>;

/** Whether a clone's lineage is big enough to fold into a bulb. */
export const foldable = (f: Forest, name: string) =>
  f.descendants(name) > BULB_MAX_FORKS || f.height(name) > BULB_MAX_DEPTH;

/** A clone and the forks it came from, up to the root: unfolding all of them shows it. */
export function lineageOf(clones: CloneInfo[], name: string): string[] {
  const { parentOf } = forest(clones);
  const out = [name];
  for (let p = parentOf.get(name); p && !out.includes(p); p = parentOf.get(p)) out.push(p);
  return out;
}

/** Whether a clone's forks are folded: by hand if it was opened or closed, else folded when its lineage is big. */
export function isFolded(f: Forest, name: string, open: ReadonlySet<string>, closed: ReadonlySet<string>) {
  if (!(f.kids.get(name) ?? []).length) return false;
  if (open.has(name)) return false;
  if (closed.has(name)) return true;
  return foldable(f, name);
}

/** A clone's forks at any depth, deepest first: the order to remove them in. */
export function forksOf(clones: CloneInfo[], name: string): CloneInfo[] {
  const f = forest(clones);
  const out: { c: CloneInfo; depth: number }[] = [];
  const walk = (n: string, depth: number) => {
    for (const k of f.kids.get(n) ?? []) {
      out.push({ c: k, depth });
      walk(k.name, depth + 1);
    }
  };
  walk(name, 1);
  return out.sort((a, b) => b.depth - a.depth).map((x) => x.c);
}
