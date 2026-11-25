// Live state of long operations, fed by backend events. Every window keeps
// its own copy, so an op started from the menu-bar popover shows up in the
// main window's activity panel too.

import type { QueryClient } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";
import { toast } from "sonner";
import { api } from "./api";
import { listen } from "./bridge";
import { ms } from "./format";
import type { CloneInfo, OpEvent, OpFinished, OpRequest, OpStarted } from "./types";

export type StepState = {
  name: string;
  start: number;
  end?: number;
  took?: number;
  skipped?: string;
  failed?: boolean;
  output: string[];
};

export type Op = {
  id: string;
  req: OpRequest;
  title: string;
  subject: string;
  startedAt: number;
  finishedAt?: number;
  status: "running" | "ok" | "error";
  error?: string;
  steps: StepState[];
  warnings: string[];
  /** Neutral notes, e.g. background servers clove stopped. */
  notes: string[];
  result?: unknown;
  dismissed: boolean;
};

const ops = new Map<string, Op>();
let snapshot: Op[] = [];
const subs = new Set<() => void>();

function emit() {
  snapshot = [...ops.values()].sort((a, b) => b.startedAt - a.startedAt);
  for (const s of subs) s();
}

function subscribe(cb: () => void) {
  subs.add(cb);
  return () => subs.delete(cb);
}

export function useOps(): Op[] {
  return useSyncExternalStore(subscribe, () => snapshot);
}

export function repoName(root: string) {
  return root.split("/").filter(Boolean).pop() ?? root;
}

export function describe(req: OpRequest): { title: string; subject: string } {
  const repo = repoName(req.root);
  switch (req.kind) {
    case "init":
      return { title: "Build golden image", subject: repo };
    case "refresh":
      return { title: "Refresh golden image", subject: repo };
    case "new":
      return {
        title: "New clone",
        subject: req.name || req.branch || (req.detach ? `detached at ${req.base ?? "base"}` : "generated name"),
      };
    case "remove":
      return { title: "Remove clone", subject: req.name };
    case "remove_worktree":
      return { title: "Remove git worktree", subject: repoName(req.path) };
    case "adopt":
      return { title: "Make a clone", subject: repoName(req.path) };
    case "deinit":
      return { title: "Delete golden image", subject: repo };
  }
}

function update(id: string, f: (op: Op) => void) {
  const op = ops.get(id);
  if (!op) return;
  const next = { ...op, steps: [...op.steps], warnings: [...op.warnings], notes: [...op.notes] };
  f(next);
  ops.set(id, next);
  emit();
}

export function dismissOp(id: string) {
  update(id, (op) => {
    op.dismissed = true;
  });
}

export function clearFinished() {
  for (const [id, op] of ops) if (op.status !== "running") ops.delete(id);
  emit();
}

export async function startOp(req: OpRequest): Promise<string> {
  return api.startOp(req);
}

let wired = false;

type Hooks = { onSuccess?: (op: Op) => void; toasts: boolean };

/** Subscribes this window to op events. Call once at startup. */
export function wireOps(qc: QueryClient, hooks: Hooks) {
  if (wired) return;
  wired = true;
  void listen<OpStarted>("op-started", (e) => {
    const { title, subject } = describe(e.op);
    ops.set(e.id, {
      id: e.id,
      req: e.op,
      title,
      subject,
      startedAt: e.at,
      status: "running",
      steps: [],
      warnings: [],
      notes: [],
      dismissed: false,
    });
    emit();
  });
  void listen<OpEvent>("op-event", ({ id, at, event }) => {
    update(id, (op) => {
      const last = (name: string) => {
        for (let i = op.steps.length - 1; i >= 0; i--) {
          if (op.steps[i].name === name) {
            op.steps[i] = { ...op.steps[i] };
            return op.steps[i];
          }
        }
        return undefined;
      };
      switch (event.event) {
        case "start":
          op.steps.push({ name: event.step, start: at, output: [] });
          break;
        case "done": {
          const s = last(event.step);
          if (s) {
            s.end = at;
            s.took = event.took;
          }
          break;
        }
        case "skip":
          op.steps.push({ name: event.step, start: at, end: at, skipped: event.reason, output: [] });
          break;
        case "output": {
          const s = last(event.step);
          if (s) s.output = [...s.output.slice(-199), event.line];
          break;
        }
        case "warn":
          op.warnings.push(event.message);
          break;
        case "info":
          op.notes.push(event.message);
          break;
      }
    });
  });
  void listen<OpFinished>("op-finished", (e) => {
    update(e.id, (op) => {
      op.finishedAt = e.at;
      op.status = e.ok ? "ok" : "error";
      op.error = e.error ?? undefined;
      op.result = e.result;
      op.steps = op.steps.map((s) => (s.end === undefined ? { ...s, end: e.at, failed: !e.ok } : s));
    });
    const op = ops.get(e.id);
    void qc.invalidateQueries();
    if (!op) return;
    if (op.status === "ok") hooks.onSuccess?.(op);
    if (!hooks.toasts) return;
    const took = ms(e.at - op.startedAt);
    const later = (op.result as { when_free?: string[] } | null)?.when_free;
    if (op.status === "ok" && later) {
      toast.info(`${op.subject} is in use`, {
        id: op.id,
        description: `By ${later.join(", ")}. clove removes it as soon as ${later.length === 1 ? "it exits" : "they exit"}.`,
      });
    } else if (op.status === "ok") {
      const clone = op.req.kind === "new" || op.req.kind === "adopt" ? (op.result as CloneInfo | null) : null;
      toast.success(doneTitle(op), {
        id: op.id,
        description:
          op.warnings.length > 0 ? `${took} · ${op.warnings.length} warning${op.warnings.length > 1 ? "s" : ""}` : took,
        action: clone?.path ? { label: "Copy path", onClick: () => void api.copy(clone.path) } : undefined,
      });
    } else {
      toast.error(`${op.title} failed`, { id: op.id, description: op.error, duration: 10_000 });
    }
  });
}

function doneTitle(op: Op): string {
  switch (op.req.kind) {
    case "init":
      return `Golden image ready for ${op.subject}`;
    case "refresh":
      return `Golden image refreshed for ${op.subject}`;
    case "new": {
      const c = op.result as CloneInfo | null;
      return `Clone ${c?.name ?? op.subject} is ready`;
    }
    case "remove":
    case "remove_worktree":
      return `Removed ${op.subject}`;
    case "adopt": {
      const c = op.result as CloneInfo | null;
      return `${op.subject} is now clone ${c?.name ?? ""}`;
    }
    case "deinit":
      return `Deleted the golden image of ${op.subject}`;
  }
}
