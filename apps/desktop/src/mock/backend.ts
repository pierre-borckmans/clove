// In-browser stand-in for the Tauri commands, with simulated operations that
// stream the same events the Rust side emits. Used by `pnpm dev` in a browser,
// the screenshot script, and later the landing page.

import { repoName } from "@/lib/ops";
import type {
  AppId,
  Check,
  Cleanup,
  CloneInfo,
  CloveEvent,
  Discovered,
  GoldenStatus,
  OpRequest,
  Refs,
  RepoConfig,
  RepoStatus,
  RepoSummary,
} from "@/lib/types";
import { CARGO_OUTPUT, type MockRepo, machineChecks, PNPM_OUTPUT, seed } from "./fixtures";

const repos = seed();
let serviceOn = true;
let shellOn = false;
const LAYERS = [
  { label: "all repos", path: () => "/Users/ada/.config/clove/config.toml" },
  { label: "team", path: (r: MockRepo) => `${r.status.root}/.clove.toml` },
  { label: "you, this repo", path: (r: MockRepo) => `/Users/ada/.config/clove/repos/${r.status.name}.toml` },
];
const listeners = new Map<string, Set<(p: unknown) => void>>();
const MB = 1024 * 1024;

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb(payload);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = () => Math.floor(Date.now() / 1000);

function get(root: string): MockRepo {
  const r = repos.get(root);
  if (!r) throw new Error(`open ${root}: No such file or directory`);
  return r;
}

function status(r: MockRepo): RepoStatus {
  const g = r.status.golden;
  return {
    ...r.status,
    golden: g ? { ...g, age_secs: now() - g.created_at } : null,
    clones: r.clones.length,
    mounted: r.clones.filter((c) => c.mounted).length,
  };
}

function sanitize(branch: string) {
  return branch.replace(/[^A-Za-z0-9._-]/g, "-").replace(/^[-.]+|[-.]+$/g, "");
}

let opSeq = 1;

type Script = { step: string; ms: number; output?: string[]; skip?: string }[];

async function play(id: string, script: Script) {
  for (const s of script) {
    const ev = (event: CloveEvent) => emit("op-event", { id, at: Date.now(), event });
    if (s.skip) {
      ev({ event: "skip", step: s.step, reason: s.skip });
      await sleep(60);
      continue;
    }
    ev({ event: "start", step: s.step });
    const t0 = Date.now();
    if (s.output?.length) {
      const per = s.ms / (s.output.length + 1);
      for (const line of s.output) {
        await sleep(per);
        ev({ event: "output", step: s.step, line });
      }
      await sleep(per);
    } else {
      await sleep(s.ms);
    }
    ev({ event: "done", step: s.step, took: Date.now() - t0 });
  }
}

function warmScript(r: MockRepo): Script {
  return r.status.steps.map((s) => ({
    step: `warm ${s.name}`,
    ms: s.name === "cargo" ? 3200 : s.name === "pnpm" ? 2400 : 500,
    output: s.name === "cargo" ? CARGO_OUTPUT : s.name === "pnpm" ? PNPM_OUTPUT : undefined,
  }));
}

async function runOp(id: string, op: OpRequest): Promise<unknown> {
  switch (op.kind) {
    case "new": {
      const r = get(op.root);
      const branch = op.detach ? null : op.branch?.trim() || `swift-otter-${Math.floor(Math.random() * 90 + 10)}`;
      const name = op.name?.trim() || sanitize(branch ?? `detached-${Date.now() % 1000}`);
      if (r.clones.some((c) => c.name === name)) {
        await sleep(120);
        throw new Error(`a clone named \`${name}\` already exists; pick another --name`);
      }
      const heavy = /bump|deps|upgrade/.test(branch ?? "");
      await play(id, [
        { step: "fetch", ms: 380 },
        { step: "clone image", ms: 24 },
        { step: "mount", ms: 290 },
        { step: "checkout", ms: 170 },
        ...r.status.steps.map((s) =>
          s.name === "pnpm" && heavy && !op.no_setup
            ? { step: "setup pnpm", ms: 1800, output: PNPM_OUTPUT }
            : { step: `setup ${s.name}`, ms: 0, skip: op.no_setup ? "--no-setup" : "unchanged since golden" },
        ),
      ]);
      const info: CloneInfo = {
        name,
        path: `${r.status.clone_root}/${name}/${r.status.name}`,
        mounted: true,
        branch,
        head: r.status.golden?.commit.slice(0, 12) ?? null,
        dirty: false,
        ahead: 0,
        behind: 0,
        image_bytes: (heavy ? 180 : 6) * MB,
        created_at: now(),
        golden_generation: r.status.golden?.generation ?? 1,
        timings: [],
        failed_steps: [],
      };
      r.clones.unshift(info);
      return info;
    }
    case "remove": {
      const r = get(op.root);
      const c = r.clones.find((x) => x.name === op.name);
      if (!c) throw new Error(`no clone named \`${op.name}\``);
      if (c.dirty && !op.force) {
        await sleep(150);
        throw new Error(
          `\`${op.name}\` has uncommitted changes; commit them, or \`clove rm --force ${op.name}\` to discard`,
        );
      }
      await play(id, [
        { step: "unmount", ms: c.mounted ? 420 : 30 },
        { step: "delete image", ms: 160 },
      ]);
      r.clones = r.clones.filter((x) => x.name !== op.name);
      if (op.delete_branch && c.branch) r.refs = r.refs.filter((x) => !(x.kind === "local" && x.name === c.branch));
      return null;
    }
    case "refresh": {
      const r = get(op.root);
      const g = r.status.golden as GoldenStatus;
      await play(id, [
        { step: "fetch", ms: 600 },
        { step: "clone golden", ms: 30 },
        { step: "mount", ms: 320 },
        { step: "checkout", ms: 900 },
        ...warmScript(r),
        { step: "index", ms: 280 },
        { step: "unmount", ms: 260 },
      ]);
      emit("op-event", {
        id,
        at: Date.now(),
        event: { event: "warn", message: `kept gen-${g.generation} until its last clone is removed` },
      });
      r.status.golden = {
        ...g,
        generation: g.generation + 1,
        commit: Array.from({ length: 40 }, () => "0123456789abcdef"[Math.floor(Math.random() * 16)]).join(""),
        created_at: now(),
        age_secs: 0,
        behind: 0,
      };
      return r.status.golden;
    }
    case "init": {
      const name = repoName(op.root);
      const r: MockRepo = {
        status: {
          name,
          root: op.root,
          id: `${name}-5d1e0c9a`,
          state: `/Users/ada/.clove/repos/${name}-5d1e0c9a`,
          clone_root: `${op.root}.clove`,
          base: "origin/main",
          backend: "asif",
          golden: null,
          steps: [
            {
              name: "pnpm",
              warm: "pnpm install",
              setup: "pnpm install --offline",
              watch: ["pnpm-lock.yaml"],
              wrap: false,
              env: {},
            },
          ],
          steps_detected: true,
        },
        clones: [],
        gc: [],
        prs: [],
        stale: null,
        layers: [],
        refs: [{ name: "origin/main", kind: "remote", age: 600, subject: "initial commit" }],
      };
      await play(id, [
        { step: "create image", ms: 700 },
        { step: "mount", ms: 300 },
        { step: "fetch", ms: 800 },
        { step: "checkout", ms: 1400 },
        ...warmScript(r),
        { step: "index", ms: 200 },
        { step: "unmount", ms: 220 },
      ]);
      r.status.golden = {
        generation: 1,
        commit: "4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f6071",
        base: "origin/main",
        created_at: now(),
        timings: [],
        steps: ["pnpm"],
        dirty: false,
        age_secs: 0,
        behind: 0,
        image_bytes: 910 * MB,
      };
      repos.set(op.root, r);
      return r.status.golden;
    }
    case "deinit": {
      const r = get(op.root);
      if (r.clones.length) throw new Error("remove the clones first (`clove ls`), or `clove rm --all`");
      await play(id, [{ step: "delete golden", ms: 500 }]);
      repos.delete(op.root);
      return null;
    }
  }
}

function startOp(op: OpRequest): string {
  const id = `op-${Date.now()}-${opSeq++}`;
  queueMicrotask(async () => {
    emit("op-started", { id, at: Date.now(), op });
    try {
      const result = await runOp(id, op);
      emit("op-finished", { id, at: Date.now(), ok: true, error: null, result });
    } catch (e) {
      emit("op-finished", {
        id,
        at: Date.now(),
        ok: false,
        error: e instanceof Error ? e.message : String(e),
        result: null,
      });
    }
  });
  return id;
}

type Args = Record<string, unknown>;

const commands: Record<string, (a: Args) => unknown> = {
  repos: (): RepoSummary[] =>
    [...repos.values()]
      .map((r) => {
        const s = status(r);
        return {
          id: s.id,
          name: s.name,
          root: s.root,
          clones: s.clones,
          mounted: s.mounted,
          generation: s.golden?.generation ?? null,
          behind: s.golden?.behind ?? null,
          stale: !!r.stale,
          refreshing: false,
          prunable: r.prs.filter((m) => m.verdict === "prune" && r.clones.some((c) => c.name === m.clone)).length,
          error: null,
        };
      })
      .sort((a, b) => a.name.localeCompare(b.name)),
  repo_status: (a) => status(get(a.root as string)),
  clones: (a) =>
    get(a.root as string).clones.map((c) => (c.mounted ? c : { ...c, dirty: null, ahead: null, behind: null })),
  refs: (a): Refs => {
    const r = get(a.root as string);
    return {
      base: r.status.base,
      refs: r.refs.map((x) => ({ name: x.name, kind: x.kind, date: now() - x.age, subject: x.subject })),
    };
  },
  doctor_machine: (): Check[] => machineChecks,
  doctor_repo: (a): Check[] => {
    const r = get(a.root as string);
    const g = r.status.golden;
    const checks: Check[] = [
      {
        name: "steps",
        level: "ok",
        detail: `${r.status.steps.map((s) => s.name).join(", ")} (${r.status.steps_detected ? "detected" : "from config"})`,
        fix: null,
      },
    ];
    if (r.status.steps.some((s) => s.name === "pnpm"))
      checks.push({
        name: "pnpm store",
        level: "warn",
        detail:
          "clove steps pin the store, but a plain `pnpm add` in a clone would start an empty store on the clone's volume",
        fix: "pnpm config set store-dir /Users/ada/Library/pnpm/store/v10 --global",
      });
    if (g)
      checks.push({
        name: "golden",
        level: (g.behind ?? 0) > 200 ? "warn" : "ok",
        detail: `gen ${g.generation} at ${g.commit.slice(0, 10)} (${g.behind ?? 0} commits behind ${g.base})`,
        fix: null,
      });
    if (r.gc.length)
      checks.push({ name: "leftovers", level: "warn", detail: r.gc.join("; "), fix: "clove gc --apply" });
    return checks;
  },
  gc: (a) => {
    const r = get(a.root as string);
    const actions = [...r.gc];
    if (a.apply) r.gc = [];
    return actions;
  },
  mount_clone: (a) => {
    const c = get(a.root as string).clones.find((x) => x.name === a.name);
    if (!c) throw new Error(`no clone named \`${a.name}\``);
    c.mounted = true;
    c.dirty = false;
    c.ahead = c.ahead ?? 0;
    c.behind = c.behind ?? 0;
    return c.path;
  },
  unmount_clone: (a) => {
    const c = get(a.root as string).clones.find((x) => x.name === a.name);
    if (!c) throw new Error(`no clone named \`${a.name}\``);
    c.mounted = false;
    return null;
  },
  discover: (a): Discovered => {
    const root = (a.dir as string).replace(/\/$/, "");
    return { root, name: repoName(root), initialized: repos.has(root), base: "origin/main", steps: ["pnpm"] };
  },
  repo_config: (a): RepoConfig => {
    const r = get(a.root as string);
    const g = r.status.golden;
    return {
      config: {
        base: r.status.base,
        clone_root: r.status.clone_root,
        image_size: "300G",
        fetch: true,
        fetch_interval: 300,
        exec: "auto",
        env: {},
        steps: r.status.steps,
        steps_detected: r.status.steps_detected,
        copy: null,
        refresh: { auto: true, max_age_secs: 86_400, max_behind: 100 },
        sources: [],
      },
      layers: LAYERS.map((l) => ({ label: l.label, path: l.path(r), exists: r.layers.includes(l.label) })),
      copy: r.status.name === "mono" ? ["apps/web/.env.local", "packages/api/.env"] : [],
      freshness: g
        ? {
            age_secs: now() - g.created_at,
            behind: g.behind,
            stale: !!r.stale,
            changed_steps: r.stale ? ["pnpm"] : [],
            reason: r.stale,
            refreshing: null,
          }
        : null,
      state_home: "/Users/ada/.clove",
    };
  },
  edit_config: (a) => {
    for (const r of repos.values())
      for (const l of LAYERS) if (l.path(r) === a.path && !r.layers.includes(l.label)) r.layers.push(l.label);
    return null;
  },
  service_status: () => serviceOn,
  service_set: (a) => {
    serviceOn = !!a.install;
    return serviceOn;
  },
  prs: (a) => get(a.root as string).prs,
  shell_status: () => ({
    shell: "zsh",
    installed: shellOn,
    rc: "/Users/ada/.zshrc",
    line: 'eval "$(clove shell-init zsh)"',
  }),
  shell_install: () => {
    shellOn = true;
    return { shell: "zsh", installed: true, rc: "/Users/ada/.zshrc", line: 'eval "$(clove shell-init zsh)"' };
  },
  cleanup: (a): Cleanup => {
    const r = get(a.root as string);
    const leftovers = [...r.gc];
    const checks = r.prs.filter((m) => r.clones.some((c) => c.name === m.clone));
    const removed: string[] = [];
    if (a.apply) {
      r.gc = [];
      for (const m of checks.filter((m) => m.verdict === "prune")) {
        r.clones = r.clones.filter((c) => c.name !== m.clone);
        removed.push(m.clone);
      }
    }
    return { leftovers, checks, warning: null, removed, kept: [] };
  },
  installed_apps: (): AppId[] => ["terminal", "ghostty", "vscode", "cursor", "zed"],
  open_in: () => null,
  copy_text: (a) => navigator.clipboard?.writeText(a.text as string).catch(() => undefined) ?? null,
  start_op: (a) => startOp(a.op as OpRequest),
  open_main: () => null,
  hide_popover: () => null,
  quit: () => null,
};

export const mockBackend = {
  async invoke<T>(cmd: string, args: Args = {}): Promise<T> {
    const f = commands[cmd];
    if (!f) throw new Error(`mock: unknown command ${cmd}`);
    await sleep(cmd === "clones" || cmd === "doctor_repo" ? 90 : cmd === "cleanup" ? 400 : 25);
    return (await f(args)) as T;
  },
  async listen<T>(event: string, cb: (p: T) => void) {
    let set = listeners.get(event);
    if (!set) {
      set = new Set();
      listeners.set(event, set);
    }
    const fn = cb as (p: unknown) => void;
    set.add(fn);
    return () => set.delete(fn);
  },
};

export async function mockPickFolder(): Promise<string | null> {
  await sleep(150);
  return "/Users/ada/src/api";
}
