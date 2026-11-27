import { useQueries, useQuery } from "@tanstack/react-query";
import { CircleCheck, CircleX, Copy, Laptop, RefreshCw, TriangleAlert } from "lucide-react";
import { PaneHeader } from "@/components/app-shell";
import { RepoMark } from "@/components/sidebar";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { api } from "@/lib/api";
import { THIS_MACHINE } from "@/lib/platform";
import { copyText, keys, useRepos } from "@/lib/queries";
import type { Check } from "@/lib/types";
import { cn } from "@/lib/utils";

export function DoctorPage() {
  const { data: repos = [] } = useRepos();
  const machine = useQuery({ queryKey: keys.doctorMachine, queryFn: api.doctorMachine });
  const perRepo = useQueries({
    queries: repos.map((r) => ({ queryKey: keys.doctorRepo(r.root), queryFn: () => api.doctorRepo(r.root) })),
  });
  const all = [machine, ...perRepo];
  const fetching = all.some((q) => q.isFetching);
  const checks = all.flatMap((q) => q.data ?? []);
  const problems = checks.filter((c) => c.level !== "ok").length;

  return (
    <>
      <PaneHeader>
        <h1 className="text-[14.5px] font-semibold tracking-tight">Doctor</h1>
        {fetching && checks.length === 0 ? (
          <span className="text-[12.5px] text-muted-foreground">Checking…</span>
        ) : problems === 0 ? (
          <span className="inline-flex h-5 items-center gap-1 rounded-full border border-sage/25 bg-sage-soft px-2 text-[11px] font-medium text-sage">
            <CircleCheck className="size-3" />
            Everything looks right
          </span>
        ) : (
          <span className="inline-flex h-5 items-center gap-1 rounded-full border border-amber/30 bg-amber/12 px-2 text-[11px] font-medium text-amber">
            <TriangleAlert className="size-3" />
            {problems} thing{problems === 1 ? "" : "s"} to look at
          </span>
        )}
        <div data-tauri-drag-region className="h-full flex-1" />
        <Button
          variant="outline"
          size="sm"
          className="h-7 gap-1.5 text-[12.5px]"
          onClick={() => {
            for (const q of all) void q.refetch();
          }}
        >
          <RefreshCw className={cn("size-3.5", fetching && "animate-spin")} />
          Run again
        </Button>
      </PaneHeader>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[860px] flex-col gap-6 px-6 pt-5 pb-16">
          <Section
            icon={<Laptop className="size-4 text-muted-foreground" />}
            title={THIS_MACHINE}
            checks={machine.data}
          />
          {repos.map((r, i) => (
            <Section
              key={r.id}
              icon={<RepoMark name={r.name} />}
              title={r.name}
              subtitle={r.root}
              checks={perRepo[i]?.data}
              error={perRepo[i]?.error}
            />
          ))}
        </div>
      </div>
    </>
  );
}

function Section({
  icon,
  title,
  subtitle,
  checks,
  error,
}: {
  icon: React.ReactNode;
  title: string;
  subtitle?: string;
  checks?: Check[];
  error?: unknown;
}) {
  return (
    <section>
      <div className="mb-2.5 flex items-center gap-2">
        {icon}
        <h2 className="text-[13.5px] font-semibold tracking-tight">{title}</h2>
        {subtitle && <span className="truncate font-mono text-[11.5px] text-faint">{subtitle}</span>}
      </div>
      <div className="clove-card overflow-hidden">
        {error ? (
          <div className="px-4 py-3 font-mono text-[12px] text-destructive">{String(error)}</div>
        ) : !checks ? (
          <div className="space-y-2 p-4">
            <Skeleton className="h-4 w-1/2" />
            <Skeleton className="h-4 w-1/3" />
          </div>
        ) : (
          checks.map((c) => <CheckRow key={c.name} check={c} />)
        )}
      </div>
    </section>
  );
}

function CheckRow({ check: c }: { check: Check }) {
  return (
    <div className="grid grid-cols-[24px_124px_minmax(0,1fr)] items-start gap-x-3 border-b border-border/70 px-4 py-3.5 transition-colors last:border-b-0 hover:bg-foreground/[0.015]">
      <span
        className={cn(
          "mt-px flex size-5 items-center justify-center rounded-full",
          c.level === "ok" && "bg-sage-soft text-sage",
          c.level === "warn" && "bg-amber/14 text-amber",
          c.level !== "ok" && c.level !== "warn" && "bg-destructive/12 text-destructive",
        )}
      >
        {c.level === "ok" ? (
          <CircleCheck className="size-3.5" />
        ) : c.level === "warn" ? (
          <TriangleAlert className="size-3.5" />
        ) : (
          <CircleX className="size-3.5" />
        )}
      </span>
      <span className="pt-px text-[13px] font-medium">{c.name}</span>
      <div className="min-w-0">
        <p className="selectable text-[13px] leading-relaxed text-muted-foreground">{c.detail}</p>
        {c.fix && (
          <div className="group mt-2 flex items-center gap-2 rounded-lg border border-border/80 bg-muted/45 py-1 pr-1 pl-2.5 shadow-2xs">
            <span className="select-none font-mono text-[12px] font-semibold text-bottle">$</span>
            <code className="selectable min-w-0 flex-1 truncate font-mono text-[12px] text-foreground/90">{c.fix}</code>
            <Button
              variant="ghost"
              size="icon-sm"
              className="size-6 text-muted-foreground hover:text-foreground"
              aria-label="Copy command"
              onClick={() => void copyText(c.fix as string, "Command copied")}
            >
              <Copy className="size-3.5" />
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
