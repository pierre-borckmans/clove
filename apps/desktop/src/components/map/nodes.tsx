import { ArrowDown, ArrowUp, GitBranch, Lock } from "lucide-react";
import type { KeyboardEvent, MouseEvent, PointerEvent } from "react";
import { CLOVE_PATH } from "@/components/glyph";
import type { CloneInfo, OtherWorktree, Pr } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ageOf, cloneState, labelSize, type StatePart, worktreeState } from "./geometry";
import type { Pull } from "./pull";

/** What a radial menu is open on. */
export type MenuTarget = { kind: "clone"; name: string } | { kind: "image" };
export type Selection =
  | { kind: "image"; generation: number }
  | { kind: "clone"; name: string }
  | { kind: "worktree"; path: string };

/** What every layout needs to draw nodes the same way. */
export type MapCtx = {
  newest: number;
  current: number | undefined;
  prOf: (clone: string) => Pr | null | undefined;
  /** The picked clone's name. */
  picked: string | undefined;
  pickedWorktree: string | undefined;
  /** The image shown in the inspector when no clone is picked. */
  imageGeneration: number | undefined;
  /** With an image picked, the age group that stays lit. */
  focusAge: number | null;
  hovered: string | null;
  setHovered: (update: (name: string | null) => string | null) => void;
  select: (selection: Selection) => void;
  open: (path: string) => void;
  /** What the radial menu is open on: a clone's name, or "image". */
  menuFor: string | undefined;
  /** Opens the radial menu on a node; it follows the node as it moves. */
  menu: (target: MenuTarget, keyboard: boolean) => void;
  /** A clove being pulled off a clone or out of the image. */
  pull: Pull | null;
  startPull: (target: MenuTarget, event: PointerEvent<Element>) => void;
  /** Clones being rewound: their skins peel back. */
  peeling: ReadonlySet<string>;
  /** Folds or unfolds a clone's forks. */
  toggleBulb: (name: string) => void;
  /** The repo's name and base ref, written on the bulb. */
  repoName: string;
  base: string;
  /** Commits the image is behind its base (0 or null: nothing to say). */
  behind: number | null;
  /** The image is being built or refreshed: a new skin forms round the bulb. */
  refreshing: boolean;
  /** The refresh's current step (fetch, checkout, setup pnpm...), shown under the bulb. */
  refreshStep: string | null;
  /** A refresh just failed: the forming skin fades out, the label says so for a few seconds. */
  refreshFailed: boolean;
  /** The oldest generation drawn in the bulb (0: just the rim). */
  deepestAge: number;
};

/** The pull on this clone, if it's the one being pulled off (not a fork's copy: the clone stays put). */
export const pullOn = (ctx: MapCtx, name: string) =>
  ctx.pull?.target.kind === "clone" && ctx.pull.target.name === name && !ctx.pull.fork ? ctx.pull : undefined;

/** A fork's copy being pulled off this clone. */
export const forkPullOn = (ctx: MapCtx, name: string) =>
  ctx.pull?.target.kind === "clone" && ctx.pull.target.name === name && ctx.pull.fork ? ctx.pull : undefined;

/** A clone's menu open on the map: every other clone dims while it's open. */
const cloneMenu = (ctx: MapCtx) => (ctx.menuFor && ctx.menuFor !== "image" ? ctx.menuFor : undefined);

/** With a clone's menu open, every other clone dims; with an image picked, clones of other ages do. */
export const isDimmed = (ctx: MapCtx, generation: number, name: string) => {
  const open = cloneMenu(ctx);
  if (open) return open !== name;
  return ctx.focusAge !== null && ageOf(ctx.newest, generation) !== ctx.focusAge;
};

export function StateLine({ parts }: { parts: StatePart[] }) {
  return parts.map((p) =>
    p.arrow ? (
      <span key={p.arrow} className="flex items-center">
        {p.arrow === "up" ? <ArrowUp className="size-2.5" /> : <ArrowDown className="size-2.5" />}
        {p.text}
      </span>
    ) : (
      <span
        key={p.text}
        className={cn(
          p.tone === "amber" && "text-amber",
          p.tone === "sage" && "text-sage",
          p.tone === "rose" && "text-rose",
        )}
      >
        {p.text}
      </span>
    ),
  );
}

export const cloneLabelSize = (c: CloneInfo, prOf: MapCtx["prOf"]) => labelSize(c.name, cloneState(c, prOf(c.name)));

const isMenuKey = (event: KeyboardEvent<Element>) =>
  event.key === "." || event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");

/**
 * Handlers that pick a node and open its radial menu: a click, a right-click,
 * Enter or Space, or period (or the menu key) from the keyboard.
 */
export function menuTrigger(ctx: MapCtx, target: MenuTarget, select: () => void) {
  return {
    onClick: () => {
      select();
      ctx.menu(target, false);
    },
    onContextMenu: (event: MouseEvent<Element>) => {
      event.preventDefault();
      select();
      ctx.menu(target, false);
    },
    onKeyDown: (event: KeyboardEvent<Element>) => {
      if (!isMenuKey(event) && event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      select();
      ctx.menu(target, true);
    },
  };
}

/** Garlic skin states, as classes on a clone node. */
export function skinClasses(c: CloneInfo, pr: Pr | null | undefined) {
  const bruised = !!(c.failed_steps.length || c.issue || c.operation);
  return cn(
    "map-piece",
    bruised ? "map-skin-bruised" : !c.mounted ? "map-skin-unmounted" : "map-skin-peeled",
    // Needs attention, then merged, then modified: one colour at a time.
    c.mounted && c.dirty && !bruised && pr?.state !== "merged" && "map-skin-modified",
    !bruised && pr?.state === "merged" && "map-skin-merged",
  );
}

/** One clove, tip pointing at `angle` degrees (0 is up), drawn at `length` px long. */
export function ClovePiece({ length, angle }: { length: number; angle: number }) {
  return (
    <svg
      viewBox="24 4 52 88"
      className="map-piece-svg"
      width={Math.round(length * 0.6)}
      height={length}
      style={{ transform: `rotate(${angle}deg)` }}
      aria-hidden="true"
    >
      <path className="clove-skin" d={CLOVE_PATH} strokeLinejoin="round" />
      <path className="clove-vein" d="M50 24 C46 44 46 64 50 80" fill="none" />
    </svg>
  );
}

/**
 * Plain git worktrees, on a shelf pinned under the map: they have no image,
 * so they take no place in its layout, and the shelf doesn't zoom.
 */
export function WorktreeShelf({ worktrees, ctx }: { worktrees: OtherWorktree[]; ctx: MapCtx }) {
  if (!worktrees.length) return null;
  return (
    <fieldset
      aria-label="Not clones yet: plain git worktrees"
      className="map-shelf absolute bottom-3 left-3 flex max-w-[calc(100%-24px)] items-center gap-0.5 overflow-x-auto rounded-xl p-1"
      onPointerDown={(event) => event.stopPropagation()}
    >
      <span className="shrink-0 px-2 text-[11px] font-medium text-muted-foreground">Not clones yet</span>
      {worktrees.map((w) => (
        <button
          type="button"
          key={w.path}
          aria-label={`Plain worktree ${w.name}`}
          aria-pressed={ctx.pickedWorktree === w.path}
          onClick={() => ctx.select({ kind: "worktree", path: w.path })}
          onDoubleClick={() => !w.missing && ctx.open(w.path)}
          className={cn(
            "map-shelf-item flex shrink-0 items-center gap-1.5 rounded-lg px-2 py-1",
            ctx.pickedWorktree === w.path && "map-node-selected",
            (ctx.focusAge !== null || !!cloneMenu(ctx)) && "map-dim",
          )}
        >
          {w.locked ? <Lock className="size-3 shrink-0" /> : <GitBranch className="size-3 shrink-0" />}
          <span className="text-[11.5px] font-medium">{w.name}</span>
          <span className="flex items-center gap-2 text-[10.5px] text-muted-foreground">
            <StateLine parts={worktreeState(w)} />
          </span>
        </button>
      ))}
    </fieldset>
  );
}
