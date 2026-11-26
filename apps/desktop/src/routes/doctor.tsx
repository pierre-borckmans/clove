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
        <h1 className="text-[14px] font-semibold tracking-tight">Doctor</h1>
        <span className="text-[12.5px] text-muted-foreground">
          {fetching && checks.length === 0
            ? "Checking…"
            : problems === 0
              ? "Everything looks right"
              : `${problems} thing${problems === 1 ? "" : "s"} to look at`}
        </span>
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
      <div className="mb-2 flex items-center gap-2">
        {icon}
        <h2 className="text-[13px] font-medium">{title}</h2>
        {subtitle && <span className="truncate font-mono text-[11.5px] text-faint">{subtitle}</span>}
      </div>
      <div className="overflow-hidden rounded-xl border bg-card">
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
    <div className="grid grid-cols-[18px_120px_minmax(0,1fr)] items-start gap-x-3 border-b px-4 py-3 last:border-b-0">
      {c.level === "ok" ? (
        <CircleCheck className="mt-px size-4 text-sage" />
      ) : c.level === "warn" ? (
        <TriangleAlert className="mt-px size-4 text-amber" />
      ) : (
        <CircleX className="mt-px size-4 text-destructive" />
      )}
      <span className="text-[13px] font-medium">{c.name}</span>
      <div className="min-w-0">
        <p className="selectable text-[13px] leading-relaxed text-muted-foreground">{c.detail}</p>
        {c.fix && (
          <div className="group mt-2 flex items-center gap-2 rounded-md border bg-muted/50 py-1 pr-1 pl-2.5">
            <span className="text-faint select-none font-mono text-[12px]">$</span>
            <code className="selectable min-w-0 flex-1 truncate font-mono text-[12px]">{c.fix}</code>
            <Button
              variant="ghost"
              size="icon-sm"
              className="size-6 text-muted-foreground"
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
