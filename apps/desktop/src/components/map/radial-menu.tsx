import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  Code,
  CopyPlus,
  FileDiff,
  FoldVertical,
  GitFork,
  GitPullRequestArrow,
  HardDrive,
  History,
  type LucideIcon,
  RefreshCw,
  Settings,
  SquareTerminal,
  Trash2,
  Undo2,
  UnfoldVertical,
  Unplug,
} from "lucide-react";
import { type CSSProperties, type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useOpenDiff } from "@/components/clone-list";
import { startOp } from "@/lib/ops";
import { APP_LABELS, usePrefs } from "@/lib/prefs";
import { checkpointNow, forkClone, openIn, useClones, useMountToggle } from "@/lib/queries";
import type { CloneInfo, RepoStatus } from "@/lib/types";
import { setBulbs, setUi } from "@/lib/ui";
import { forksOf } from "./lineage";

/** `tone` is the action's colour, a CSS colour (usually a palette token). */
export type RadialAction = { label: string; icon: LucideIcon; tone: string; run: () => void; disabled?: boolean };

const TONE = {
  open: "var(--tone-blue)",
  terminal: "var(--tone-violet)",
  changes: "var(--amber)",
  grow: "var(--bottle)",
  disk: "var(--tone-teal)",
  remove: "var(--rose)",
  time: "var(--sage)",
  neutral: "var(--muted-foreground)",
};

/** A clone's main actions, as in the list view's menu. */
export function useCloneActions(
  c: CloneInfo,
  root: string,
  /** Its folded (or unfolded) forks on the map, in bulb mode. */
  bulb?: { forks: number; open: boolean },
  /** It and the clones it was forked from: a fork unfolds them all, so it shows. */
  lineage: string[] = [c.name],
): RadialAction[] {
  const prefs = usePrefs();
  const mount = useMountToggle(root);
  const openDiff = useOpenDiff(root);
  const qc = useQueryClient();
  const all = useClones(root).data;
  const n = c.checkpoints ?? 0;
  return [
    { label: "View changes", icon: FileDiff, tone: TONE.changes, run: () => openDiff(c), disabled: !c.mounted },
    {
      label: `Open in ${APP_LABELS[prefs.terminal]}`,
      icon: SquareTerminal,
      tone: TONE.terminal,
      run: () => void openIn(c.path, prefs.terminal),
    },
    {
      label: `Open in ${APP_LABELS[prefs.editor]}`,
      icon: Code,
      tone: TONE.open,
      run: () => void openIn(c.path, prefs.editor),
    },
    { label: "Checkpoint", icon: History, tone: TONE.time, run: () => void checkpointNow(qc, root, c.name) },
    // Rewind only once there is something to rewind to.
    ...(n
      ? [
          {
            label: `Rewind… (${n} checkpoint${n === 1 ? "" : "s"})`,
            icon: Undo2,
            tone: TONE.time,
            run: () => setUi({ rewind: { root, clone: c } }),
          },
        ]
      : []),
    {
      label: "Fork (or ⌥-drag the clove)",
      icon: GitFork,
      tone: TONE.grow,
      run: () => {
        setBulbs(root, lineage, true);
        forkClone(root, c.name);
      },
    },
    ...(bulb
      ? [
          {
            label: bulb.open ? "Hide forks" : `Show forks (${bulb.forks})`,
            icon: bulb.open ? FoldVertical : UnfoldVertical,
            tone: TONE.grow,
            run: () => setBulbs(root, [c.name], !bulb.open),
          },
        ]
      : []),
    {
      label: "Remove clone…",
      icon: Trash2,
      tone: TONE.remove,
      run: () => setUi({ remove: { root, clones: [c], forks: forksOf(all ?? [], c.name) } }),
    },
    {
      label: c.mounted ? "Unmount" : "Mount",
      icon: c.mounted ? Unplug : HardDrive,
      tone: TONE.disk,
      run: () => mount.mutate({ name: c.name, mounted: c.mounted }),
    },
    c.operation
      ? {
          label: "Continue update",
          icon: GitPullRequestArrow,
          tone: TONE.grow,
          run: () => void startOp({ kind: "update", root, name: c.name, step: "continue" }),
        }
      : {
          label: "Update to latest base",
          icon: GitPullRequestArrow,
          tone: TONE.grow,
          run: () => void startOp({ kind: "update", root, name: c.name }),
          disabled: !c.branch || !c.mounted,
        },
  ];
}

/** The golden image's main actions: make a clone, refresh, and the repo itself. */
export function useImageActions(status: RepoStatus, refreshing: boolean): RadialAction[] {
  const prefs = usePrefs();
  const navigate = useNavigate();
  return [
    { label: "New clone…", icon: CopyPlus, tone: TONE.grow, run: () => setUi({ newClone: { root: status.root } }) },
    {
      label: refreshing ? "Refreshing…" : "Refresh image",
      icon: RefreshCw,
      tone: TONE.disk,
      run: () => void startOp({ kind: "refresh", root: status.root }),
      disabled: refreshing || !status.golden,
    },
    {
      label: `Open repo in ${APP_LABELS[prefs.editor]}`,
      icon: Code,
      tone: TONE.open,
      run: () => void openIn(status.root, prefs.editor),
    },
    {
      label: `Open repo in ${APP_LABELS[prefs.terminal]}`,
      icon: SquareTerminal,
      tone: TONE.terminal,
      run: () => void openIn(status.root, prefs.terminal),
    },
    {
      label: "Repo settings",
      icon: Settings,
      tone: TONE.neutral,
      run: () => void navigate({ to: "/settings", search: { repo: status.id } }),
    },
  ];
}

/** How long the ring takes to go. */
const CLOSE_MS = 180;

/**
 * Actions in a ring around a node. Drawn over the map in screen space, so it
 * keeps its size at any zoom; `x`/`y` are viewport pixels and `hole` the radius
 * left clear for the node itself.
 */
export function RadialMenu({
  title,
  actions,
  x,
  y,
  hole,
  keyboard,
  onClose,
  closing,
  onClosed,
  showTitle = true,
}: {
  title: string;
  actions: RadialAction[];
  x: number;
  y: number;
  hole: number;
  /** Opened from the keyboard: focus moves into the ring. */
  keyboard: boolean;
  onClose: (refocus: boolean) => void;
  /** Animate out, then call onClosed. */
  closing: boolean;
  onClosed: () => void;
  /**
   * Draw the title above the ring. Off for a clone: its own map label rises
   * into that place instead (scene.setLift), so nothing is swapped.
   */
  showTitle?: boolean;
}) {
  const [hint, setHint] = useState<string | null>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);

  // Closing: the ring, its buttons and pills fade and shrink away, then the
  // menu is removed. (A clone's label drops back by itself, in CSS.)
  const root = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when closing starts.
  useLayoutEffect(() => {
    if (!closing) return;
    const el = root.current;
    if (!el || matchMedia("(prefers-reduced-motion: reduce)").matches) return onClosed();
    const animations = [
      ...el.querySelectorAll<HTMLElement>(".map-radial-disc, .map-radial-item, .map-radial-hint"),
    ].map((part) =>
      part.animate([{ opacity: 1 }, { opacity: 0, scale: "0.85" }], {
        duration: CLOSE_MS,
        easing: "cubic-bezier(0.4, 0, 0.6, 1)",
        fill: "forwards",
      }),
    );
    const done = setTimeout(onClosed, CLOSE_MS);
    return () => {
      clearTimeout(done);
      for (const a of animations) a.cancel();
    };
  }, [closing]);
  const reach = hole + 24;
  const outer = reach + 26;

  // From the keyboard, focus the first action that can run, as a menu does.
  useEffect(() => {
    if (keyboard) items.current.find((el) => el && !el.disabled)?.focus({ preventScroll: true });
  }, [keyboard]);

  // Opened with the mouse, focus isn't in the ring: Escape still closes it.
  useEffect(() => {
    if (closing) return;
    const onEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      onClose(true);
    };
    window.addEventListener("keydown", onEscape);
    return () => window.removeEventListener("keydown", onEscape);
  }, [closing, onClose]);

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose(true);
      return;
    }
    // Arrows walk round the ring, skipping actions that can't run.
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (!step) return;
    event.preventDefault();
    event.stopPropagation();
    const at = items.current.indexOf(document.activeElement as HTMLButtonElement);
    for (let i = 1; i <= actions.length; i++) {
      const next = items.current[(at + step * i + actions.length * 2) % actions.length];
      if (next && !next.disabled) return next.focus({ preventScroll: true });
    }
  };

  return (
    <div
      ref={root}
      role="menu"
      aria-label={`Actions for ${title}`}
      className="map-radial absolute z-20"
      style={
        {
          left: x,
          top: y,
          "--hole": `${hole}px`,
          "--outer": `${outer}px`,
          pointerEvents: closing ? "none" : undefined,
        } as CSSProperties
      }
      onKeyDown={onKeyDown}
      onPointerDown={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
    >
      <span className="map-radial-disc" aria-hidden="true" />
      {actions.map((action, i) => {
        const angle = -Math.PI / 2 + (i * Math.PI * 2) / actions.length;
        const Icon = action.icon;
        return (
          <button
            key={action.label}
            ref={(el) => {
              items.current[i] = el;
            }}
            type="button"
            role="menuitem"
            aria-label={action.label}
            disabled={action.disabled}
            className="map-radial-item flex items-center justify-center rounded-full"
            style={
              {
                "--dx": `${Math.cos(angle) * reach}px`,
                "--dy": `${Math.sin(angle) * reach}px`,
                "--i": i,
                "--tone": action.tone,
              } as CSSProperties
            }
            onPointerEnter={() => setHint(action.label)}
            onPointerLeave={() => setHint(null)}
            onFocus={() => setHint(action.label)}
            onClick={() => {
              action.run();
              onClose(false);
            }}
          >
            <Icon className="size-4" />
          </button>
        );
      })}
      {showTitle && (
        <span
          className="map-radial-hint map-radial-name absolute whitespace-nowrap rounded-full px-2.5 py-1 text-[12px] font-semibold"
          aria-hidden="true"
        >
          {title}
        </span>
      )}
      {hint && (
        <span
          className="map-radial-hint map-radial-action absolute whitespace-nowrap rounded-full px-2.5 py-1 text-[12px] font-medium"
          aria-hidden="true"
        >
          {hint}
        </span>
      )}
    </div>
  );
}

type Placement = {
  x: number;
  y: number;
  hole: number;
  keyboard: boolean;
  onClose: (refocus: boolean) => void;
  closing: boolean;
  onClosed: () => void;
};

export function CloneRadialMenu({
  clone,
  root,
  bulb,
  lineage,
  ...place
}: { clone: CloneInfo; root: string; bulb?: { forks: number; open: boolean }; lineage?: string[] } & Placement) {
  return (
    <RadialMenu title={clone.name} actions={useCloneActions(clone, root, bulb, lineage)} showTitle={false} {...place} />
  );
}

export function ImageRadialMenu({
  status,
  refreshing,
  ...place
}: { status: RepoStatus; refreshing: boolean } & Placement) {
  const title = status.golden ? `Bulb (golden image) · gen ${status.golden.generation}` : "Bulb (golden image)";
  return <RadialMenu title={title} actions={useImageActions(status, refreshing)} {...place} />;
}
