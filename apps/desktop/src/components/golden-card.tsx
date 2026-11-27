import { ArrowDown, Check, Copy, Loader, RefreshCw, Sparkles, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ago, bytes, ms, shortSha } from "@/lib/format";
import { startOp, useOps } from "@/lib/ops";
import { copyText } from "@/lib/queries";
import type { RepoStatus } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Glyph } from "./glyph";

const STEP_SHADES = ["bg-bottle", "bg-peel", "bg-bottle/60", "bg-foreground/45", "bg-foreground/25"];

export function GoldenCard({ status }: { status: RepoStatus }) {
  const ops = useOps();
  const refreshing = ops.find(
    (o) => o.status === "running" && o.req.root === status.root && (o.req.kind === "refresh" || o.req.kind === "init"),
  );
  const g = status.golden;

  if (!g) {
    return (
      <section className="clove-card clove-golden-header relative overflow-hidden p-5">
        <div
          aria-hidden="true"
          className="absolute inset-x-0 top-0 h-[2px] bg-gradient-to-r from-bottle via-peel to-transparent opacity-80"
        />
        <div className="flex items-center gap-3.5">
          <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-bottle/20 bg-card text-bottle shadow-xs">
            <Glyph className="size-5" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-[13.5px] font-semibold tracking-tight">No golden image yet</div>
            <div className="mt-0.5 text-[12.5px] text-muted-foreground">
              Build it once; every clone of <span className="font-medium text-foreground/90">{status.name}</span> starts
              from it in about a second.
            </div>
          </div>
          <Button size="sm" disabled={!!refreshing} onClick={() => void startOp({ kind: "init", root: status.root })}>
            {refreshing ? <Loader className="size-3.5 animate-spin" /> : <Sparkles className="size-3.5" />}
            {refreshing ? "Building…" : "Build golden image"}
          </Button>
        </div>
      </section>
    );
  }

  const behind = g.behind ?? 0;
  const total = g.timings.reduce((a, t) => a + t.ms, 0);
  const warm = g.timings.filter((t) => t.step.startsWith("warm "));
  const rest = total - warm.reduce((a, t) => a + t.ms, 0);

  return (
    <section className="clove-card relative overflow-hidden">
      <div
        aria-hidden="true"
        className="absolute inset-x-0 top-0 h-[2px] bg-gradient-to-r from-bottle via-peel to-transparent opacity-85"
      />
      <div className="clove-golden-header flex items-center gap-2.5 border-b px-4 py-3">
        <span className="flex size-7 items-center justify-center rounded-lg border border-bottle/20 bg-card text-bottle shadow-2xs">
          <Glyph className="size-4" />
        </span>
        <div className="flex items-center gap-2">
          <span className="text-[13.5px] font-semibold tracking-tight">Golden image</span>
          <span className="inline-flex h-5 items-center rounded-full border border-peel/35 bg-peel-soft px-2 font-mono text-[11px] font-medium text-foreground/85 tabular-nums">
            gen {g.generation}
          </span>
        </div>
        {g.dirty && (
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="inline-flex h-5 items-center gap-1 rounded-full border border-amber/30 bg-amber/12 px-2 text-[11px] font-medium text-amber">
                <TriangleAlert className="size-3" /> dirty
              </span>
            </TooltipTrigger>
            <TooltipContent>Warm steps modified tracked files; every clone starts with those changes.</TooltipContent>
          </Tooltip>
        )}
        <div className="flex-1" />
        <Button
          variant="outline"
          size="sm"
          className="h-7 gap-1.5 px-2.5 text-[12.5px]"
          disabled={!!refreshing}
          onClick={() => void startOp({ kind: "refresh", root: status.root })}
        >
          {refreshing ? (
            <Loader className="size-3.5 animate-spin [animation-duration:1.4s]" />
          ) : (
            <RefreshCw className="size-3.5" />
          )}
          {refreshing ? "Refreshing…" : "Refresh"}
        </Button>
      </div>

      <div className="grid grid-cols-5 divide-x divide-border/75">
        <Stat label="Commit">
          <button
            type="button"
            className="group -mx-1.5 inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 font-mono text-[13px] font-medium transition-colors hover:bg-bottle-soft hover:text-bottle"
            onClick={() => void copyText(g.commit, "Commit copied")}
            title="Copy full commit"
          >
            <span>{shortSha(g.commit, 10)}</span>
            <Copy className="size-3 text-faint opacity-0 transition-opacity group-hover:opacity-100" />
          </button>
        </Stat>
        <Stat label="Base">
          <span className="truncate font-mono text-[13px] font-medium text-foreground/90">{g.base}</span>
        </Stat>
        <Stat label="Behind base">
          {behind > 0 ? (
            <span className="inline-flex items-center gap-1 rounded-full border border-bottle/20 bg-bottle-soft px-2 py-0.5 text-[12px] font-medium text-bottle">
              <ArrowDown className="size-3" />
              <span className="font-mono tabular-nums">{behind}</span>
              <span className="font-normal opacity-80">commit{behind === 1 ? "" : "s"}</span>
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 rounded-full border border-sage/20 bg-sage-soft px-2 py-0.5 text-[12px] font-medium text-sage">
              <Check className="size-3" strokeWidth={2.5} />
              Up to date
            </span>
          )}
        </Stat>
        <Stat label="Built">
          <span
            className="text-[13px] font-medium text-foreground/90"
            title={new Date(g.created_at * 1000).toLocaleString()}
          >
            {ago(g.age_secs)}
          </span>
        </Stat>
        <Stat label="Image">
          <div className="flex items-center gap-1.5">
            <span className="font-mono text-[13px] font-medium tabular-nums">{bytes(g.image_bytes)}</span>
            <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] font-medium tracking-wider text-muted-foreground uppercase">
              {status.backend}
            </span>
          </div>
        </Stat>
      </div>

      <div className="border-t border-border/75 bg-muted/25 px-4 py-3">
        <div className="mb-2 flex items-baseline justify-between">
          <span className="flex items-center gap-1.5">
            <span className="clove-label">Steps</span>
            <span className="text-[11.5px] text-faint">
              · {status.steps_detected ? "detected" : "from .clove.toml"}
            </span>
          </span>
          {total > 0 && (
            <span className="text-[12px] text-muted-foreground">
              built in <span className="font-mono font-medium tabular-nums text-foreground/90">{ms(total)}</span>
            </span>
          )}
        </div>
        {total > 0 && (
          <div className="mb-3 flex h-2 gap-[2px] overflow-hidden rounded-full bg-foreground/[0.06] p-[1px]">
            {warm.map((t, i) => (
              <Tooltip key={t.step}>
                <TooltipTrigger asChild>
                  <span
                    className={cn(
                      "h-full rounded-[2px] transition-opacity first:rounded-l-full hover:opacity-85",
                      STEP_SHADES[i % STEP_SHADES.length],
                    )}
                    style={{ width: `${(t.ms / total) * 100}%` }}
                  />
                </TooltipTrigger>
                <TooltipContent>
                  {t.step} · {ms(t.ms)}
                </TooltipContent>
              </Tooltip>
            ))}
            <span
              className="h-full flex-1 rounded-r-full bg-foreground/[0.12]"
              title={`checkout, fetch and image work · ${ms(rest)}`}
            />
          </div>
        )}
        <div className="flex flex-wrap gap-1.5">
          {warm.map((t, i) => (
            <StepChip
              key={t.step}
              shade={STEP_SHADES[i % STEP_SHADES.length]}
              name={t.step.replace(/^warm /, "")}
              time={ms(t.ms)}
              detail={status.steps.find((s) => `warm ${s.name}` === t.step)}
            />
          ))}
          {status.steps
            .filter((s) => !g.timings.some((t) => t.step === `warm ${s.name}`))
            .map((s) => (
              <StepChip key={s.name} shade="bg-foreground/25" name={s.name} detail={s} />
            ))}
          {status.steps.length === 0 && (
            <span className="text-[12.5px] text-muted-foreground">
              No steps. Clones get the checkout only; add <code className="font-mono">[[step]]</code> to{" "}
              <code className="font-mono">.clove.toml</code>.
            </span>
          )}
        </div>
      </div>
    </section>
  );
}

function Stat({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5 px-4 py-3">
      <span className="clove-label">{label}</span>
      <div className="flex min-w-0 items-center">{children}</div>
    </div>
  );
}

function StepChip({
  name,
  time,
  shade,
  detail,
}: {
  name: string;
  time?: string;
  shade: string;
  detail?: { warm?: string | null; setup?: string | null; watch: string[] };
}) {
  const chip = (
    <span className="inline-flex h-6 items-center gap-1.5 rounded-full border border-border/90 bg-card px-2.5 text-[12px] shadow-2xs transition-colors hover:border-foreground/20">
      <span className={cn("size-1.5 rounded-full", shade)} />
      <span className="font-medium text-foreground/90">{name}</span>
      {time && <span className="font-mono text-[11px] tabular-nums text-muted-foreground">{time}</span>}
    </span>
  );
  if (!detail) return chip;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{chip}</TooltipTrigger>
      <TooltipContent className="max-w-sm">
        <div className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 font-mono text-[11px]">
          {detail.warm && (
            <>
              <span className="opacity-60">warm</span>
              <span>{detail.warm}</span>
            </>
          )}
          {detail.setup && (
            <>
              <span className="opacity-60">setup</span>
              <span>{detail.setup}</span>
            </>
          )}
          {detail.watch.length > 0 && (
            <>
              <span className="opacity-60">watch</span>
              <span>{detail.watch.join(", ")}</span>
            </>
          )}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}
