// The stats `clove_core::stats` computes, for the browser mock.
import type { CloneInfo, Created, RepoStats, Stats } from "@/lib/types";

const RERAN_MS = 1000;
const FAST_MS = 5000;

function pct(values: number[], p: number): number | null {
  if (!values.length) return null;
  const v = [...values].sort((a, b) => a - b);
  return v[Math.min(v.length, Math.max(1, Math.ceil((p / 100) * v.length))) - 1];
}

const setup = (c: Created) => c.timings.filter((t) => t.step.startsWith("setup ")).reduce((n, t) => n + t.ms, 0);
const untimed = (c: Created) => Math.max(0, c.total_ms - c.timings.reduce((n, t) => n + t.ms, 0));

export function mockStats(repo: string, root: string, clones: CloneInfo[]): RepoStats {
  const created: Created[] = clones
    .map((c) => ({
      at: c.created_at,
      name: c.name,
      branch: c.branch,
      base: "origin/main",
      golden_generation: c.golden_generation,
      total_ms: c.total_ms ?? c.timings.reduce((n, t) => n + t.ms, 0),
      timings: c.timings,
      failed_steps: c.failed_steps,
    }))
    .sort((a, b) => b.at - a.at);
  const totals = created.map((c) => c.total_ms);
  const order: string[] = [];
  for (const c of [...created].reverse()) for (const t of c.timings) if (!order.includes(t.step)) order.push(t.step);
  const stat = (step: string, values: number[]) => ({
    step,
    runs: values.length,
    median_ms: pct(values, 50) ?? 0,
    p90_ms: pct(values, 90) ?? 0,
    max_ms: Math.max(0, ...values),
  });
  const steps = order.map((s) =>
    stat(
      s,
      created.flatMap((c) => c.timings.filter((t) => t.step === s).map((t) => t.ms)),
    ),
  );
  if (created.length) steps.push(stat("untimed", created.map(untimed)));
  const stats: Stats = {
    clones: created.length,
    median_ms: pct(totals, 50) ?? 0,
    p90_ms: pct(totals, 90) ?? 0,
    fastest_ms: totals.length ? Math.min(...totals) : 0,
    slowest_ms: Math.max(0, ...totals),
    fast: totals.filter((t) => t < FAST_MS).length,
    reran: created.filter((c) => setup(c) > RERAN_MS).length,
    clean_median_ms: pct(
      created.filter((c) => setup(c) <= RERAN_MS).map((c) => c.total_ms),
      50,
    ),
    steps,
    golden_builds: 0,
    golden_median_ms: null,
  };
  return { repo, root, stats, created, golden_builds: [] };
}
