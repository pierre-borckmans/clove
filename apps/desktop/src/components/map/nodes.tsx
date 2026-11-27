import { ArrowDown, ArrowUp, Check, GitBranch, Lock } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { CLOVE_PATH } from "@/components/glyph";
import type { CloneInfo, OtherWorktree, Pr } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ageOf, cloneState, type LabelSpot, labelSize, type StatePart, worktreeState } from "./geometry";

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
};

export const isDimmed = (ctx: MapCtx, generation: number) =>
  ctx.focusAge !== null && ageOf(ctx.newest, generation) !== ctx.focusAge;

/** A name and state line at a computed world box, positioned relative to its node. */
function Label({
  name,
  state,
  tag,
  at,
  origin,
}: {
  name: string;
  state: ReactNode;
  tag?: string;
  at: LabelSpot;
  origin: { x: number; y: number };
}) {
  const align = at.align;
  return (
    <span
      className={cn(
        "map-leaf-label absolute flex flex-col",
        align === "left" && "items-start text-left",
        align === "right" && "items-end text-right",
        align === "center" && "items-center text-center",
      )}
      style={{ left: at.x - origin.x, top: at.y - origin.y, width: at.w }}
    >
      <span className="map-clone-name flex max-w-full items-baseline gap-1.5 rounded px-1.5 py-0.5 text-[12px] font-medium">
        <span className="truncate">{name}</span>
        {tag && <span className="map-gen-tag shrink-0 font-mono text-[10px] font-normal">{tag}</span>}
      </span>
      <span className="map-clone-state mt-0.5 flex h-4 items-center justify-center gap-2 text-[10.5px] text-muted-foreground">
        {state}
      </span>
    </span>
  );
}

function StateLine({ parts }: { parts: StatePart[] }) {
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

export const cloneLabelSize = (c: CloneInfo, prOf: MapCtx["prOf"], tag?: string) =>
  labelSize(c.name, cloneState(c, prOf(c.name)), tag);

/** A clone on the map: a button holding a symbol, with its name and state beside it. */
export function CloneNode({
  clone: c,
  ctx,
  x,
  y,
  size,
  label,
  tag,
  leaving,
  className,
  style,
  children,
}: {
  clone: CloneInfo;
  ctx: MapCtx;
  /** Centre of the button, and its square size. */
  x: number;
  y: number;
  size: number;
  label: LabelSpot;
  tag?: string;
  /** On its way out: shown while it animates, but no longer interactive. */
  leaving?: boolean;
  className?: string;
  style?: CSSProperties;
  children: ReactNode;
}) {
  const warning = !!(c.failed_steps.length || c.issue || c.operation);
  const origin = { x: x - size / 2, y: y - size / 2 };
  return (
    <button
      type="button"
      inert={leaving}
      aria-label={`Clone ${c.name}`}
      aria-pressed={ctx.picked === c.name}
      title={c.branch ?? c.name}
      onClick={() => ctx.select({ kind: "clone", name: c.name })}
      onPointerEnter={() => ctx.setHovered(() => c.name)}
      onPointerLeave={() => ctx.setHovered((h) => (h === c.name ? null : h))}
      onDoubleClick={() => c.mounted && ctx.open(c.path)}
      className={cn(
        "map-clone absolute",
        ctx.picked === c.name && "map-node-selected",
        isDimmed(ctx, c.golden_generation) && "map-dim",
        !c.mounted && "map-clone-unmounted",
        warning && "map-clone-warning",
        className,
      )}
      style={{ left: origin.x, top: origin.y, width: size, height: size, ...style }}
    >
      {children}
      <Label
        name={c.name}
        tag={tag}
        at={label}
        origin={origin}
        state={<StateLine parts={cloneState(c, ctx.prOf(c.name))} />}
      />
    </button>
  );
}

/** Garlic skin states, as classes on a clone node. */
export function skinClasses(c: CloneInfo, pr: Pr | null | undefined) {
  const bruised = !!(c.failed_steps.length || c.issue || c.operation);
  return cn(
    "map-piece",
    bruised ? "map-skin-bruised" : !c.mounted ? "map-skin-unmounted" : "map-skin-peeled",
    c.mounted && c.dirty && !bruised && "map-skin-modified",
    pr?.state === "merged" && "map-skin-merged",
  );
}

/** One clove, tip pointing at `angle` degrees (0 is up), drawn at `length` px long. */
export function ClovePiece({ length, angle, cut }: { length: number; angle: number; cut?: boolean }) {
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
      {cut && <path className="clove-cut" d="M37 56 L60 44" fill="none" strokeLinecap="round" />}
    </svg>
  );
}

/** The small check a merged clone carries. */
export function MergedBadge({ className, style }: { className?: string; style?: CSSProperties }) {
  return (
    <span
      className={cn("map-merged-badge absolute flex size-3.5 items-center justify-center rounded-full", className)}
      style={style}
    >
      <Check className="size-2.5" strokeWidth={3} />
    </span>
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
          title={w.path}
          onClick={() => ctx.select({ kind: "worktree", path: w.path })}
          onDoubleClick={() => !w.missing && ctx.open(w.path)}
          className={cn(
            "map-shelf-item flex shrink-0 items-center gap-1.5 rounded-lg px-2 py-1",
            ctx.pickedWorktree === w.path && "map-node-selected",
            ctx.focusAge !== null && "map-dim",
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
