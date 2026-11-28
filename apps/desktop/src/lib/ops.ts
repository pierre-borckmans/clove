// Live state of long operations, fed by backend events. Every window keeps
// its own copy, so an op started from the menu-bar popover shows up in the
// main window's activity panel too.

import type { QueryClient } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";
import { toast } from "sonner";
import { api } from "./api";
import { listen } from "./bridge";
import { ms } from "./format";
import { getPrefs } from "./prefs";
import type { CloneInfo, OpEvent, OpFinished, OpRequest, OpStarted, Rewound, UpdateResult } from "./types";

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
    case "fork":
      return { title: "Fork clone", subject: req.from };
    case "rewind":
      return { title: req.force ? "Rewind (forced)" : "Rewind clone", subject: `${req.name} to ${req.id ?? "latest"}` };
    case "update":
      return {
        title: req.step === "continue" ? "Finish update" : req.step === "abort" ? "Abort update" : "Update clone",
        subject: req.name,
      };
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

/** Takes a finished operation off the list, Activity included (its ✕). */
export function forgetOp(id: string) {
  if (ops.delete(id)) emit();
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

type Hooks = {
  onSuccess?: (op: Op) => void;
  toasts: boolean;
  /** This window shows op cards (OpsPanel): they say done or failed, so plain toasts would repeat them. */
  opCards?: boolean;
};

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
    const upd = op.req.kind === "update" ? (op.result as UpdateResult | null) : null;
    if (op.status === "ok" && upd?.result === "conflict" && op.req.kind === "update") {
      const { root, name } = op.req;
      void api.repoConfig(root).then((cfg) => {
        const resolver = cfg.config.update?.resolver;
        toast.warning(`${op.subject}: ${upd.strategy} stopped on conflicts`, {
          id: `${op.id}:note`,
          duration: 30_000,
          description: `${upd.files.join(", ")}. Fix them and git add them, then Continue update from the clone's menu (or Abort).`,
          action: resolver
            ? { label: `Resolve with ${resolverName(resolver)}`, onClick: () => resolveInTerminal(root, name) }
            : undefined,
        });
      });
    } else if (op.status === "ok" && upd?.result === "updated" && (upd.hint || upd.failed_steps.length)) {
      toast.success(doneTitle(op), {
        id: `${op.id}:note`,
        description: [upd.failed_steps.length ? `setup failed: ${upd.failed_steps.join(", ")}` : "", upd.hint ?? ""]
          .filter(Boolean)
          .join(" · "),
      });
    } else if (op.status === "error" && op.req.kind === "rewind" && !op.req.force && busyUsers(op.error)) {
      // Rewinding unmounts the clone: say who holds it, and offer to go anyway.
      // That toast is the whole story, so the failed op card goes.
      const req = op.req;
      forgetOp(op.id);
      toast.warning(`${req.name} is in use`, {
        id: `${op.id}:note`,
        duration: 20_000,
        description: `By ${busyUsers(op.error)}. Rewinding anyway unmounts it under them: a shell inside then needs \`cd $PWD\`, and editors may lose unsaved buffers.`,
        action: { label: "Rewind anyway", onClick: () => void startOp({ ...req, force: true }) },
      });
    } else if (op.status === "ok" && later) {
      toast.info(`${op.subject} is in use`, {
        id: `${op.id}:note`,
        description: `By ${later.join(", ")}. clove removes it as soon as ${later.length === 1 ? "it exits" : "they exit"}.`,
      });
    } else if (hooks.opCards) {
      // The op's own card turns into the done (or failed) message.
      return;
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

/** The processes in clove's "the volume is busy: zsh (pid 1), …; close it or pass --force". */
export function busyUsers(error: string | undefined): string | null {
  const m = error?.match(/volume is busy: (.*?)(?:; close it or pass --force)?$/);
  return m ? m[1] : null;
}

/** "Claude" for `claude --model opus` (clove_core::update::resolver_name). */
export function resolverName(resolver: string): string {
  const word = (resolver.trim().split(/\s+/)[0] ?? "resolver").split("/").pop() ?? "resolver";
  return word.charAt(0).toUpperCase() + word.slice(1);
}

/** Opens the preferred terminal in the clone with the resolver running. */
export function resolveInTerminal(root: string, name: string) {
  api
    .resolveInTerminal(root, name, getPrefs().terminal)
    .catch((e) => toast.error(`Could not start the resolver for ${name}`, { description: String(e) }));
}

export function doneTitle(op: Op): string {
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
    case "fork": {
      const c = op.result as CloneInfo | null;
      return `Forked ${op.req.from} into ${c?.name ?? "a new clone"}`;
    }
    case "rewind": {
      const w = op.result as Rewound | null;
      return `${op.req.name} is back at ${w?.to ?? op.req.id ?? "its checkpoint"}`;
    }
    case "update": {
      const u = op.result as UpdateResult | null;
      if (!u) return `${op.subject} is back where it was`;
      if (u.result === "up_to_date") return `${op.subject} is up to date with ${u.base}`;
      if (u.result === "updated")
        return `${op.subject} is on ${u.base}${u.commits ? ` (${u.commits} new commit${u.commits === 1 ? "" : "s"})` : ""}`;
      return `${op.subject} has conflicts`;
    }
  }
}
