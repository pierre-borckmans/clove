import { ArrowDown, Loader, RefreshCw, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ago, bytes, ms, shortSha } from "@/lib/format";
import { startOp, useOps } from "@/lib/ops";
import { copyText } from "@/lib/queries";
import type { RepoStatus } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Glyph } from "./glyph";

const STEP_SHADES = ["bg-rust", "bg-sage", "bg-amber", "bg-foreground/45", "bg-foreground/25"];

export function GoldenCard({ status }: { status: RepoStatus }) {
  const ops = useOps();
  const refreshing = ops.find(
    (o) => o.status === "running" && o.req.root === status.root && (o.req.kind === "refresh" || o.req.kind === "init"),
  );
  const g = status.golden;

  if (!g) {
    return (
      <section className="rounded-xl border bg-card p-5">
        <div className="flex items-center gap-3">
          <Glyph className="size-5 text-faint" />
          <div className="flex-1">
            <div className="text-[13px] font-medium">No golden image</div>
            <div className="text-[12.5px] text-muted-foreground">
              Build it once; every clone of {status.name} starts from it.
            </div>
          </div>
          <Button size="sm" disabled={!!refreshing} onClick={() => void startOp({ kind: "init", root: status.root })}>
            Build golden image
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
    <section className="overflow-hidden rounded-xl border bg-card shadow-[0_1px_2px_0_rgb(0_0_0/0.03)]">
      <div className="flex items-center gap-2.5 border-b px-4 py-2.5">
        <span className="flex size-6 items-center justify-center rounded-md bg-rust-soft text-rust">
          <Glyph className="size-3.5" />
        </span>
        <span className="text-[13px] font-medium">Golden image</span>
        <span className="rounded-full border px-1.5 font-mono text-[11px] leading-[18px] text-muted-foreground">
          gen {g.generation}
        </span>
        {g.dirty && (
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="flex items-center gap-1 text-[12px] text-amber">
                <TriangleAlert className="size-3.5" /> dirty
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

      <div className="grid grid-cols-5 divide-x">
        <Stat label="Commit">
          <button
            type="button"
            className="font-mono text-[13px] hover:text-rust"
            onClick={() => void copyText(g.commit, "Commit copied")}
            title="Copy full commit"
          >
            {shortSha(g.commit, 10)}
          </button>
        </Stat>
        <Stat label="Base">
          <span className="truncate font-mono text-[13px]">{g.base}</span>
        </Stat>
        <Stat label="Behind base">
          {behind > 0 ? (
            <span className="flex items-center gap-1 text-[13px] text-rust">
              <ArrowDown className="size-3.5" />
              <span className="font-mono tabular-nums">{behind}</span>
              <span className="text-muted-foreground">commit{behind === 1 ? "" : "s"}</span>
            </span>
          ) : (
            <span className="text-[13px] text-sage">Up to date</span>
          )}
        </Stat>
        <Stat label="Built">
          <span className="text-[13px]" title={new Date(g.created_at * 1000).toLocaleString()}>
            {ago(g.age_secs)}
          </span>
        </Stat>
        <Stat label="Image">
          <span className="font-mono text-[13px] tabular-nums">{bytes(g.image_bytes)}</span>
          <span className="ml-1.5 text-[11.5px] uppercase text-faint">{status.backend}</span>
        </Stat>
      </div>

      <div className="border-t px-4 py-3">
        <div className="mb-2 flex items-baseline justify-between">
          <span className="text-[12px] text-muted-foreground">
            Steps <span className="text-faint">· {status.steps_detected ? "detected" : "from .clove.toml"}</span>
          </span>
          {total > 0 && (
            <span className="text-[12px] text-muted-foreground">
              built in <span className="font-mono tabular-nums text-foreground/80">{ms(total)}</span>
            </span>
          )}
        </div>
        {total > 0 && (
          <div className="mb-3 flex h-1.5 gap-px overflow-hidden rounded-full bg-foreground/[0.05]">
            {warm.map((t, i) => (
              <Tooltip key={t.step}>
                <TooltipTrigger asChild>
                  <span
                    className={cn("h-full first:rounded-l-full", STEP_SHADES[i % STEP_SHADES.length])}
                    style={{ width: `${(t.ms / total) * 100}%` }}
                  />
                </TooltipTrigger>
                <TooltipContent>
                  {t.step} · {ms(t.ms)}
                </TooltipContent>
              </Tooltip>
            ))}
            <span
              className="h-full flex-1 bg-foreground/[0.12]"
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
    <div className="flex min-w-0 flex-col gap-1 px-4 py-3">
      <span className="text-[11.5px] text-muted-foreground">{label}</span>
      <div className="flex min-w-0 items-baseline">{children}</div>
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
    <span className="inline-flex h-6 items-center gap-1.5 rounded-md border bg-background px-2 text-[12px]">
      <span className={cn("size-1.5 rounded-full", shade)} />
      <span className="font-medium">{name}</span>
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
