import { Link } from "@tanstack/react-router";
import { Activity, Check, ChevronDown, Copy, Loader, TriangleAlert, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useHotkey } from "@/hooks/use-hotkey";
import { useNow } from "@/hooks/use-now";
import { api } from "@/lib/api";
import { ms } from "@/lib/format";
import { dismissOp, doneTitle, forgetOp, type Op, useOps } from "@/lib/ops";
import type { CloneInfo } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";
import { cn } from "@/lib/utils";
import { barsFromSteps, Waterfall } from "./waterfall";

/** How long a finished card stays, from when its op ended. */
const DONE_FOR = 3000;

/** Floating stack of live operations, bottom right of the main pane. */
export function OpsPanel() {
  const ops = useOps();
  const { activity, addRepo } = useUi();
  const visible = (activity ? ops : ops.filter((o) => !o.dismissed)).slice(0, 4);
  /** A card under the pointer stays until the pointer leaves. */
  const [held, setHeld] = useState<string | null>(null);

  // A finished card says done briefly, then goes; one just left gets a moment.
  useEffect(() => {
    const timers = ops
      .filter((o) => o.status === "ok" && !o.dismissed && !activity && o.id !== held)
      .map((o) => setTimeout(() => dismissOp(o.id), Math.max(1200, DONE_FOR - (Date.now() - (o.finishedAt ?? 0)))));
    return () => {
      for (const t of timers) clearTimeout(t);
    };
  }, [ops, activity, held]);

  useHotkey({ key: "Escape" }, () => setUi({ activity: false }), activity);

  // Operations live in memory, so a fresh session has none: say so rather
  // than open nothing.
  if (addRepo || (visible.length === 0 && !activity)) return null;
  return (
    <div className="pointer-events-none absolute right-4 bottom-4 z-40 flex w-[380px] flex-col gap-2">
      {activity && (
        <div className="pointer-events-auto flex items-center justify-between rounded-lg border bg-popover py-1 pr-1 pl-3 text-[12px] text-muted-foreground shadow-sm backdrop-blur-xl">
          <span>Recent activity</span>
          <button
            type="button"
            className="rounded px-1.5 py-0.5 hover:bg-accent hover:text-foreground"
            onClick={() => {
              for (const o of ops) dismissOp(o.id);
              setUi({ activity: false });
            }}
          >
            Hide
          </button>
        </div>
      )}
      {visible.map((op) => (
        <OpCard key={op.id} op={op} onHold={(on) => setHeld((h) => (on ? op.id : h === op.id ? null : h))} />
      ))}
      {activity && visible.length === 0 && (
        <div className="pointer-events-auto flex gap-3 rounded-xl border bg-popover px-3.5 py-3 shadow-[0_8px_30px_-6px_rgb(0_0_0/0.18),0_2px_6px_-2px_rgb(0_0_0/0.08)] backdrop-blur-xl animate-in fade-in-0 slide-in-from-bottom-2 duration-300">
          <Activity className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div className="text-[12.5px] leading-relaxed text-muted-foreground">
            Nothing has run since the app opened. New clones, refreshes, updates and removals show up here as they run.{" "}
            <Link
              to="/history"
              onClick={() => setUi({ activity: false })}
              className="text-foreground underline decoration-foreground/30 underline-offset-2 hover:decoration-foreground"
            >
              History
            </Link>{" "}
            lists removed clones.
          </div>
        </div>
      )}
    </div>
  );
}

export function OpCard({ op, embedded, onHold }: { op: Op; embedded?: boolean; onHold?: (held: boolean) => void }) {
  const running = op.status === "running";
  const now = useNow(running);
  const [open, setOpen] = useState(true);
  const end = op.finishedAt ?? now;
  const total = end - op.startedAt;
  const bars = barsFromSteps(op.steps, op.startedAt, end);
  const current = [...op.steps].reverse().find((s) => s.end === undefined);
  const tail = current?.output.slice(-2) ?? [];
  const clone =
    op.status === "ok" && (op.req.kind === "new" || op.req.kind === "adopt") ? (op.result as CloneInfo | null) : null;

  // The steps list keeps to a few rows and follows new ones, unless scrolled up.
  const steps = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs as steps arrive.
  useEffect(() => {
    const el = steps.current;
    if (el && follow.current) el.scrollTop = el.scrollHeight;
  }, [op.steps.length]);

  return (
    <div
      onPointerEnter={() => onHold?.(true)}
      onPointerLeave={() => onHold?.(false)}
      className={cn(
        "pointer-events-auto overflow-hidden",
        !embedded &&
          "rounded-xl border bg-popover shadow-[0_8px_30px_-6px_rgb(0_0_0/0.18),0_2px_6px_-2px_rgb(0_0_0/0.08)] backdrop-blur-xl animate-in fade-in-0 slide-in-from-bottom-2 duration-300",
      )}
    >
      {!embedded && (
        <div className="flex items-center gap-2.5 px-3.5 pt-3 pb-2.5">
          <StatusIcon status={op.status} />
          <div className="min-w-0 flex-1 leading-tight">
            <div className="truncate text-[13px] font-medium">
              {op.status === "ok" ? (
                doneTitle(op)
              ) : (
                <>
                  {op.title}{" "}
                  <span className="font-mono text-[12px] font-normal text-muted-foreground">{op.subject}</span>
                </>
              )}
            </div>
          </div>
          <span className="font-mono text-[11.5px] tabular-nums text-muted-foreground">{ms(total)}</span>
          {clone?.path && (
            <button
              type="button"
              aria-label="Copy path"
              title="Copy path"
              onClick={() => void api.copy(clone.path)}
              className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              <Copy className="size-3.5" />
            </button>
          )}
          <button
            type="button"
            aria-label={open ? "Collapse" : "Expand"}
            onClick={() => setOpen(!open)}
            className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <ChevronDown className={cn("size-3.5 transition-transform", !open && "-rotate-90")} />
          </button>
          {!running && (
            <button
              type="button"
              aria-label="Dismiss"
              onClick={() => forgetOp(op.id)}
              className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          )}
        </div>
      )}
      {open && (
        <div className={cn(!embedded && "border-t px-3.5 pt-2.5 pb-3")}>
          {bars.length > 0 ? (
            <div
              ref={steps}
              className={cn(!embedded && "max-h-[150px] overflow-y-auto")}
              onScroll={(event) => {
                const el = event.currentTarget;
                follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < 8;
              }}
            >
              <Waterfall bars={bars} total={total} dense />
            </div>
          ) : (
            <div className="py-1 text-[12px] text-muted-foreground">Starting…</div>
          )}
          {running && tail.length > 0 && (
            <div className="mt-2.5 rounded-md bg-foreground/[0.04] px-2 py-1.5 font-mono text-[11px] leading-[1.5] text-muted-foreground">
              {tail.map((l, i) => (
                <div key={`${i}-${l}`} className="truncate">
                  {l}
                </div>
              ))}
            </div>
          )}
          {op.notes.map((n) => (
            <div key={n} className="selectable mt-2 text-[12px] leading-snug text-muted-foreground">
              {n}
            </div>
          ))}
          {op.warnings.map((w) => (
            <div key={w} className="mt-2 flex gap-1.5 text-[12px] leading-snug text-amber">
              <TriangleAlert className="mt-px size-3.5 shrink-0" />
              <span className="selectable">{w}</span>
            </div>
          ))}
          {op.error && (
            <div className="selectable mt-2.5 rounded-md border border-destructive/25 bg-destructive/[0.06] px-2.5 py-2 font-mono text-[11.5px] leading-relaxed text-destructive">
              {op.error}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function StatusIcon({ status }: { status: Op["status"] }) {
  if (status === "running")
    return (
      <span className="flex size-[18px] items-center justify-center rounded-full bg-bottle-soft text-bottle">
        <Loader className="size-3 animate-spin [animation-duration:1.4s]" />
      </span>
    );
  if (status === "ok")
    return (
      <span className="flex size-[18px] items-center justify-center rounded-full bg-sage-soft text-sage animate-in zoom-in-50 duration-200">
        <Check className="size-3" strokeWidth={3} />
      </span>
    );
  return (
    <span className="flex size-[18px] items-center justify-center rounded-full bg-destructive/12 text-destructive animate-in zoom-in-50 duration-200">
      <X className="size-3" strokeWidth={3} />
    </span>
  );
}
