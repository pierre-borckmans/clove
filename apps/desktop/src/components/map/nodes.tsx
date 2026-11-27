import { ArrowDown, ArrowUp, Check, GitBranch, Lock } from "lucide-react";
import type { CSSProperties, KeyboardEvent, MouseEvent, ReactNode } from "react";
import { CLOVE_PATH } from "@/components/glyph";
import type { CloneInfo, OtherWorktree, Pr } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ageOf, cloneState, type LabelSpot, labelSize, type StatePart, worktreeState } from "./geometry";

/** What a radial menu is open on. */
export type MenuTarget = { kind: "clone"; name: string } | { kind: "image" };
/** Where a menu opens, in viewport client pixels, and the radius of the node it surrounds (0 for none). */
export type MenuAnchor = {
  x: number;
  y: number;
  radius: number;
  /** Where the node's name sits on screen, for the menu's title to grow out of. */
  title?: ScreenRect;
};
export type ScreenRect = { x: number; y: number; w: number; h: number };

/** Where a label's name sits on screen. */
export function nameRect(name: Element | null | undefined): ScreenRect | undefined {
  if (!name) return undefined;
  const box = name.getBoundingClientRect();
  return { x: box.left, y: box.top, w: box.width, h: box.height };
}

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
  /** How far the open clone's label rises (world units) to sit above its ring. */
  menuLift: { dx: number; dy: number } | undefined;
  menu: (target: MenuTarget, at: MenuAnchor, keyboard: boolean) => void;
};

/** A clone's menu open on the map: every other clone dims while it's open. */
const cloneMenu = (ctx: MapCtx) => (ctx.menuFor && ctx.menuFor !== "image" ? ctx.menuFor : undefined);

/** With a clone's menu open, every other clone dims; with an image picked, clones of other ages do. */
export const isDimmed = (ctx: MapCtx, generation: number, name: string) => {
  const open = cloneMenu(ctx);
  if (open) return open !== name;
  return ctx.focusAge !== null && ageOf(ctx.newest, generation) !== ctx.focusAge;
};

/** A name and state line at a computed world box, positioned relative to its node. */
function Label({
  name,
  state,
  at,
  origin,
  lift,
}: {
  name: string;
  state: ReactNode;
  at: LabelSpot;
  origin: { x: number; y: number };
  lift?: { dx: number; dy: number };
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
      style={{
        left: at.x - origin.x,
        top: at.y - origin.y,
        width: at.w,
        transform: lift ? `translate(${lift.dx}px, ${lift.dy}px)` : undefined,
      }}
    >
      <span className="map-clone-name flex max-w-full items-baseline gap-1.5 rounded px-1.5 py-0.5 text-[12px] font-medium">
        <span className="truncate">{name}</span>
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

export const cloneLabelSize = (c: CloneInfo, prOf: MapCtx["prOf"]) => labelSize(c.name, cloneState(c, prOf(c.name)));

/** A clone on the map: a button holding a symbol, with its name and state beside it. */
export function CloneNode({
  clone: c,
  ctx,
  x,
  y,
  size,
  label,
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
      {...menuTrigger(ctx, { kind: "clone", name: c.name }, () => ctx.select({ kind: "clone", name: c.name }))}
      onPointerEnter={() => ctx.setHovered(() => c.name)}
      onPointerLeave={() => ctx.setHovered((h) => (h === c.name ? null : h))}
      className={cn(
        "map-clone absolute",
        ctx.picked === c.name && "map-node-selected",
        ctx.menuFor === c.name && "map-menu-open",
        leaving && "map-leaving",
        isDimmed(ctx, c.golden_generation, c.name) && "map-dim",
        !c.mounted && "map-clone-unmounted",
        warning && "map-clone-warning",
        className,
      )}
      // Placed with a transform, not left/top: cheaper to move every frame.
      style={{
        left: 0,
        top: 0,
        width: size,
        height: size,
        transform: `translate(${origin.x}px, ${origin.y}px)`,
        ...style,
      }}
    >
      {children}
      <Label
        name={c.name}
        at={label}
        origin={origin}
        state={<StateLine parts={cloneState(c, ctx.prOf(c.name))} />}
        lift={ctx.menuFor === c.name ? ctx.menuLift : undefined}
      />
    </button>
  );
}

/**
 * Where a menu opens. A clone's opens on its clove; one whose button is
 * bigger than a clove (a bulb slice) opens at the pointer, or its label from
 * the keyboard. The image's centres on it, with a hole it fits in.
 */
function menuAnchor(target: MenuTarget, button: HTMLElement, pointer?: { clientX: number; clientY: number }) {
  const box = button.getBoundingClientRect();
  const centre = { x: box.left + box.width / 2, y: box.top + box.height / 2 };
  const radius = Math.min(box.width, box.height) / 2;
  const title = nameRect(button.querySelector(".map-clone-name"));
  // Judge "a clove" by its size on the map, not on screen, which grows with zoom.
  if (target.kind === "image") return { ...centre, radius };
  if (button.offsetWidth <= 60) return { ...centre, radius, title };
  if (pointer?.clientX) return { x: pointer.clientX, y: pointer.clientY, radius: 0 };
  const label = button.querySelector(".map-leaf-label")?.getBoundingClientRect() ?? box;
  return { x: label.left + label.width / 2, y: label.top + label.height / 2, radius: 0 };
}

const isMenuKey = (event: KeyboardEvent<HTMLElement>) =>
  event.key === "." || event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");

/**
 * Handlers that pick a node and open its radial menu: a click, a right-click,
 * or period (or the menu key) from the keyboard.
 */
export function menuTrigger(ctx: MapCtx, target: MenuTarget, select: () => void) {
  return {
    onClick: (event: MouseEvent<HTMLElement>) => {
      select();
      // A click from Enter or Space has no pointer: focus goes into the ring.
      ctx.menu(target, menuAnchor(target, event.currentTarget, event), event.detail === 0);
    },
    onContextMenu: (event: MouseEvent<HTMLElement>) => {
      event.preventDefault();
      select();
      ctx.menu(target, menuAnchor(target, event.currentTarget, event), false);
    },
    onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
      if (!isMenuKey(event)) return;
      event.preventDefault();
      select();
      ctx.menu(target, menuAnchor(target, event.currentTarget), true);
    },
  };
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
