import { ms } from "@/lib/format";
import type { StepState } from "@/lib/ops";
import type { Timing } from "@/lib/types";
import { cn } from "@/lib/utils";

export type Bar = {
  name: string;
  offset: number;
  duration: number;
  state: "done" | "running" | "skipped" | "failed";
  note?: string;
};

export function barsFromSteps(steps: StepState[], start: number, now: number): Bar[] {
  return steps.map((s) => ({
    name: s.name,
    offset: s.start - start,
    duration: (s.end ?? now) - s.start,
    state: s.skipped ? "skipped" : s.failed ? "failed" : s.end === undefined ? "running" : "done",
    note: s.skipped,
  }));
}

export function barsFromTimings(timings: Timing[]): Bar[] {
  let t = 0;
  return timings.map((x) => {
    const b: Bar = { name: x.step, offset: t, duration: x.ms, state: "done" };
    t += x.ms;
    return b;
  });
}

export function Waterfall({
  bars,
  total,
  dense,
  className,
}: {
  bars: Bar[];
  total: number;
  dense?: boolean;
  className?: string;
}) {
  const span = Math.max(total, 1);
  return (
    <div
      className={cn(
        "grid grid-cols-[minmax(76px,max-content)_minmax(0,1fr)_52px] items-center gap-x-3",
        dense ? "gap-y-1" : "gap-y-1.5",
        className,
      )}
    >
      {bars.map((b, i) => {
        const left = (b.offset / span) * 100;
        const width = (b.duration / span) * 100;
        return (
          <div key={`${b.name}-${i}`} className="contents animate-in fade-in-0 slide-in-from-left-1 duration-200">
            <span
              className={cn(
                "truncate font-mono text-[11.5px]",
                b.state === "skipped" ? "text-faint" : "text-muted-foreground",
                b.state === "running" && "text-foreground",
                b.state === "failed" && "text-destructive",
              )}
            >
              {b.name}
            </span>
            <div className="relative h-[5px] rounded-full bg-foreground/[0.055]">
              {b.state === "skipped" ? (
                <span
                  className="absolute top-1/2 size-[5px] -translate-y-1/2 rounded-full border border-faint"
                  style={{ left: `${Math.min(left, 99)}%` }}
                />
              ) : (
                <span
                  className={cn(
                    "absolute inset-y-0 min-w-[3px] rounded-full transition-[width] duration-100 ease-linear",
                    b.state === "done" && "bg-sage",
                    b.state === "failed" && "bg-destructive",
                    b.state === "running" &&
                      "animate-shimmer bg-bottle bg-[length:24px_100%] bg-[linear-gradient(90deg,transparent_0,rgb(255_255_255/0.35)_50%,transparent_100%)]",
                  )}
                  style={{ left: `${left}%`, width: `${Math.min(width, 100 - left)}%` }}
                />
              )}
            </div>
            <span
              className={cn(
                "text-right font-mono text-[11.5px] tabular-nums",
                b.state === "skipped" ? "text-faint" : "text-muted-foreground",
              )}
              title={b.note}
            >
              {b.state === "skipped" ? "skip" : ms(b.duration)}
            </span>
          </div>
        );
      })}
    </div>
  );
}
