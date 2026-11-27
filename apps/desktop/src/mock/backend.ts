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
  FieldKind,
  FieldState,
  GoldenStatus,
  InitPreview,
  Layer,
  OpRequest,
  Refs,
  RepoCandidate,
  RepoConfig,
  RepoRef,
  RepoStatus,
  RepoSummary,
} from "@/lib/types";
import { MOCK_ATTRS, MOCK_PACKAGES, MOCK_PATCH } from "./diff";
import { CARGO_OUTPUT, type MockRepo, machineChecks, PNPM_OUTPUT, seed, worktreeCheck } from "./fixtures";
import { mockStats } from "./stats";

const repos = seed();
let serviceOn = true;
let agentsState: "installed" | "outdated" | "missing" = "missing";
let shellOn = false;
const LAYERS = [
  { label: "all repos", path: () => "/Users/ada/.config/clove/config.toml" },
  { label: "team", path: (r: MockRepo) => `${r.status.root}/.clove.toml` },
  { label: "you, this repo", path: (r: MockRepo) => `/Users/ada/.config/clove/repos/${r.status.name}.toml` },
];
// Mirrors clove_core::config::FIELDS for the browser mock.
const FIELDS: [string, string, string, FieldKind, string | null, string][] = [
  ["base", "base", "clones", { type: "text" }, null, "ref new clones branch from, e.g. origin/main"],
  ["clone_root", "live in", "clones", { type: "text" }, null, "where clones mount; {repo}, {repo_parent} and ~ expand"],
  ["image_size", "image size", "clones", { type: "size" }, "300G", "maximum size of each image"],
  ["fetch", "fetch", "clones", { type: "bool" }, "true", "fetch the base ref before creating a clone"],
  ["fetch_interval", "fetch at most", "clones", { type: "seconds" }, "5m", "skip that fetch when recent, e.g. 5m"],
  [
    "exec",
    "environment",
    "clones",
    { type: "choice", options: ["auto", "direnv", "mise", "none"] },
    "auto",
    "wrap step commands in direnv or mise",
  ],
  ["env", "env", "clones", { type: "map" }, "", "extra environment: NAME=value, ..."],
  ["copy", "local files", "clones", { type: "list" }, "detected", "files copied into clones; comma separated"],
  ["detect", "detect steps", "steps", { type: "bool" }, "true", "detect steps when none are configured"],
  ["refresh.auto", "auto refresh", "freshness", { type: "bool" }, "true", "refresh when a new clone finds it stale"],
  ["refresh.max_age", "max age", "freshness", { type: "duration" }, "24h", "stale after this long, e.g. 12h, 2d"],
  ["refresh.max_behind", "max behind", "freshness", { type: "int" }, "100", "stale this many commits behind"],
  ["prune.auto", "auto prune", "cleanup", { type: "bool" }, "false", "the service removes merged, clean, idle clones"],
];
const LAYER_IDS: Layer[] = ["all", "team", "you"];
const settings = new Map<string, Record<string, string>[]>();

function mockFields(r: MockRepo): FieldState[] {
  const set = settings.get(r.status.root) ?? [{}, {}, {}];
  return FIELDS.map(([key, label, section, kind, def, help]) => {
    const layers = set.map((l) => l[key] ?? null);
    const at = [2, 1, 0].find((i) => layers[i] !== null);
    const resolved = key === "base" ? r.status.base : r.status.clone_root;
    return {
      key,
      label,
      section,
      kind,
      help,
      default: def === null ? "resolved" : { value: def },
      layers,
      effective: at === undefined ? (def ?? resolved) : (layers[at] as string),
      source: at === undefined ? null : LAYER_IDS[at],
    };
  });
}

function mockSet(root: string, layer: Layer, key: string, value: string | null): string {
  const f = FIELDS.find((x) => x[0] === key);
  if (!f) throw new Error(`unknown setting "${key}"`);
  const kind = f[3];
  const v = value?.trim();
  if (v !== undefined) {
    if (kind.type === "choice" && !kind.options.includes(v))
      throw new Error(`${key}: use one of ${kind.options.join(", ")}`);
    if (kind.type === "bool" && !["true", "false"].includes(v)) throw new Error(`${key}: use true or false`);
    if (kind.type === "int" && !/^\d+$/.test(v)) throw new Error(`${key}: use a whole number`);
    if ((kind.type === "duration" || kind.type === "seconds") && !/^\d+[smhd]?$/.test(v))
      throw new Error(`${key}: bad duration (use 30m, 12h, 2d)`);
    if (kind.type === "size" && !/^\d+[KMGTkmgt]$/.test(v)) throw new Error(`${key}: use K, M, G or T, e.g. 300G`);
  }
  const set = settings.get(root) ?? [{}, {}, {}];
  settings.set(root, set);
  const l = set[LAYER_IDS.indexOf(layer)];
  if (v === undefined) {
    if (!(key in l)) return `${key} isn't set there`;
    delete l[key];
    return `${key} reset`;
  }
  l[key] = v;
  return `${key} = ${v}`;
}

const listeners = new Map<string, Set<(p: unknown) => void>>();
const MB = 1024 * 1024;

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb(payload);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
// `?slow=3000` makes the git-backed reads take that long, to see loading states.
const SLOW_MS = Number(new URLSearchParams(globalThis.location?.search).get("slow")) || 0;
const SLOW = new Set(SLOW_MS ? ["repos", "repo_status", "clones", "worktrees"] : []);
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
    case "remove_worktree": {
      const r = get(op.root);
      const w = r.worktrees.find((x) => x.path === op.path);
      if (!w) throw new Error(`${op.path} is not a plain git worktree of ${r.status.name}`);
      const overridden = w.can_force && (!w.locked || op.unlock) && (w.dirty !== true || op.force);
      if (w.blocker && !overridden) {
        await sleep(120);
        throw new Error(w.blocker);
      }
      await play(id, [{ step: op.force ? "git worktree remove --force" : "git worktree remove", ms: 260 }]);
      r.worktrees = r.worktrees.filter((x) => x.path !== op.path);
      if (op.delete_branch && w.branch) r.refs = r.refs.filter((x) => !(x.kind === "local" && x.name === w.branch));
      return null;
    }
    case "update": {
      const r = get(op.root);
      const c = r.clones.find((x) => x.name === op.name);
      if (!c) throw new Error(`no clone named \`${op.name}\``);
      if (op.step === "abort") {
        await play(id, [{ step: "rebase --abort", ms: 200 }]);
        c.operation = undefined;
        return null;
      }
      if (op.step === "continue") {
        await play(id, [{ step: "rebase --continue", ms: 400 }]);
        c.operation = undefined;
        c.behind = 0;
        return {
          result: "updated",
          base: "origin/main",
          strategy: "rebase",
          commits: null,
          hint: null,
          failed_steps: [],
        };
      }
      if (c.dirty) throw new Error(`\`${c.name}\` has uncommitted changes; commit them, or update with autostash`);
      const n = c.behind ?? 0;
      if (!n) return { result: "up_to_date", base: "origin/main" };
      await play(id, [
        { step: "fetch", ms: 380 },
        { step: `rebase ${c.branch} onto origin/main`, ms: 520 },
      ]);
      if (c.name.includes("billing")) {
        c.operation = "rebase";
        return { result: "conflict", base: "origin/main", strategy: "rebase", files: ["apps/web/src/billing.ts"] };
      }
      c.behind = 0;
      await play(id, [{ step: "setup pnpm", ms: 900 }]);
      return { result: "updated", base: "origin/main", strategy: "rebase", commits: n, hint: null, failed_steps: [] };
    }
    case "adopt": {
      const r = get(op.root);
      const w = r.worktrees.find((x) => x.path === op.path);
      if (!w) throw new Error(`${op.path} is not a plain git worktree of ${r.status.name}`);
      const why = w.adopt_blocker ?? (w.locked ? `${w.name} is locked` : null);
      if (why || !w.branch) {
        await sleep(120);
        throw new Error(why ?? `${w.name} is detached; a clone takes a branch`);
      }
      const name = op.name?.trim() || sanitize(w.branch);
      emit("op-event", {
        id,
        at: Date.now(),
        event: { event: "info", message: `detaching ${w.name} so the clone can take ${w.branch}` },
      });
      await play(id, [
        { step: "fetch", ms: 380 },
        { step: "clone image", ms: 24 },
        { step: "mount", ms: 290 },
        { step: "checkout", ms: 170 },
        ...r.status.steps.map((s) => ({ step: `setup ${s.name}`, ms: 0, skip: "unchanged since golden" })),
      ]);
      const info: CloneInfo = {
        name,
        path: `${r.status.clone_root}/${name}/${r.status.name}`,
        mounted: true,
        branch: w.branch,
        head: w.head,
        dirty: false,
        ahead: 0,
        behind: 0,
        image_bytes: 6 * MB,
        created_at: now(),
        golden_generation: r.status.golden?.generation ?? 1,
        timings: [],
        failed_steps: [],
      };
      r.clones.unshift(info);
      r.worktrees = r.worktrees.filter((x) => x.path !== op.path);
      return info;
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
        worktrees: [],
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
  repo_list: (): RepoRef[] =>
    [...repos.values()]
      .map((r) => ({ id: r.status.id, name: r.status.name, root: r.status.root }))
      .sort((a, b) => a.name.localeCompare(b.name)),
  repo_status: (a) => status(get(a.root as string)),
  clones: (a) =>
    get(a.root as string).clones.map((c) => (c.mounted ? c : { ...c, dirty: null, ahead: null, behind: null })),
  worktrees: (a) => get(a.root as string).worktrees,
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
  repo_candidates: (): RepoCandidate[] =>
    MOCK_CANDIDATES.filter(([root]) => !repos.has(root)).map(([root, branch, ago]) => ({
      root,
      name: repoName(root),
      branch,
      active: now() - ago,
    })),
  init_preview: (a): InitPreview => {
    const root = (a.dir as string).replace(/^~/, "/Users/ada").replace(/\/$/, "");
    if (root.split("/").length < 4) throw new Error(`not inside a git repository: ${root}`);
    const name = repoName(root);
    const tauri = name === "desk";
    return {
      root,
      name,
      initialized: repos.has(root),
      base: "origin/main",
      base_found: true,
      steps: [
        { name: "mise-trust", warm: "mise trust --quiet --yes .", setup: "mise trust --quiet --yes .", tool: "mise" },
        { name: "mise", warm: "mise install --yes", setup: null, tool: "mise" },
        { name: "pnpm", warm: "pnpm install --frozen-lockfile", setup: null, tool: "pnpm" },
        ...(tauri
          ? [
              {
                name: "cargo-src-tauri",
                warm: "cd src-tauri && cargo build --workspace --all-targets",
                setup: null,
                tool: "cargo",
              },
            ]
          : []),
      ],
      steps_detected: true,
      nested_skipped: name === "monorepo" ? ["apps/a", "apps/b", "apps/c", "packages/ui", "packages/db"] : [],
      copy: [".env.local"],
      exec: "mise",
      image_size: "300G",
      clone_root: `${root}.clove`,
      state: `/Users/ada/.clove/repos/${name}-5d1e0c9a`,
      missing_tools: tauri ? ["cargo"] : [],
      machine: [],
    };
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
        prune: { auto: false },
        update: { strategy: "rebase", resolver: "claude" },
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
      fields: mockFields(r),
    };
  },
  config_set: (a) => mockSet(a.root as string, a.layer as Layer, a.key as string, (a.value as string | null) ?? null),
  edit_config: (a) => {
    for (const r of repos.values())
      for (const l of LAYERS) if (l.path(r) === a.path && !r.layers.includes(l.label)) r.layers.push(l.label);
    return null;
  },
  agents_status: () => agentsState,
  agents_set: (a) => {
    agentsState = a.install ? "installed" : "missing";
    return agentsState;
  },
  stats: () => [...repos.values()].map((r) => mockStats(r.status.name, r.status.root, r.clones)),
  history: () => [
    {
      repo: "mono",
      root: "/Users/ada/code/mono",
      at: now() - 3 * 3600,
      kind: "clone",
      name: "fix-login-redirect",
      path: "/Users/ada/code/mono.clove/fix-login-redirect/mono",
      branch: "fix/login-redirect",
      head: "4be2c19d0a77",
      by: "auto-prune",
      reason: "PR #1423 merged",
      branch_deleted: true,
      restore: "git branch fix/login-redirect 4be2c19d0a77",
    },
  ],
  resolve_in_terminal: () => null,
  service_status: () => serviceOn,
  service_set: (a) => {
    serviceOn = !!a.install;
    return serviceOn;
  },
  prs: (a) => get(a.root as string).prs,
  check_prs: (a) => get(a.root as string).prs,
  branch_candidates: (a) =>
    get(a.root as string)
      .refs.filter((r) => r.kind !== "tag" && !/^(main|master|HEAD)$/.test(r.name.replace(/^origin\//, "")))
      .map((r) => ({ name: r.name.replace(/^origin\//, ""), remote: r.kind === "remote" })),
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
    const worktrees = r.worktrees.map(worktreeCheck);
    const removed: string[] = [];
    const removed_worktrees: string[] = [];
    if (a.apply) {
      r.gc = [];
      for (const m of checks.filter((m) => m.verdict === "prune")) {
        r.clones = r.clones.filter((c) => c.name !== m.clone);
        removed.push(m.clone);
      }
      for (const m of worktrees.filter((m) => m.verdict === "prune")) {
        r.worktrees = r.worktrees.filter((w) => w.path !== m.path);
        removed_worktrees.push(m.path);
      }
    }
    return { leftovers, checks, worktrees, warning: null, removed, removed_worktrees, kept: [] };
  },
  installed_apps: (): AppId[] => ["terminal", "ghostty", "vscode", "cursor", "zed"],
  open_in: () => null,
  copy_text: (a) => navigator.clipboard?.writeText(a.text as string).catch(() => undefined) ?? null,
  start_op: (a) => startOp(a.op as OpRequest),
  open_main: () => null,
  hide_popover: () => null,
  quit: () => null,
  forget_cache: () => null,
  clone_diff: (a) => {
    // `?diff=<url>`: a real clove_core::CloneDiff as JSON, to try the view on a real branch.
    const src = new URLSearchParams(location.search).get("diff");
    if (src) return fetch(src).then((r) => r.json());
    const c = get(a.root as string).clones.find((x) => x.name === a.name);
    const dirty = !!(c?.dirty || c?.ahead);
    return {
      base: "origin/main",
      merge_base: "a41c0e9d3b7f",
      patch: dirty ? MOCK_PATCH : "",
      truncated: false,
      skipped: [],
      attrs: dirty ? MOCK_ATTRS : [],
      packages: dirty ? MOCK_PACKAGES : [],
    };
  },
  clone_file: () => "",
};

// Repos "on disk" the add-repo dialog offers: root, branch, seconds since last use.
const MOCK_CANDIDATES: [string, string | null, number][] = [
  ["/Users/ada/src/api", "main", 900],
  ["/Users/ada/code/desk", "feat/tray-menu", 3 * 3600],
  ["/Users/ada/Documents/Dev/monorepo", "main", 2 * 86_400],
  ["/Users/ada/projects/notes", null, 40 * 86_400],
];

export const mockBackend = {
  async invoke<T>(cmd: string, args: Args = {}): Promise<T> {
    const f = commands[cmd];
    if (!f) throw new Error(`mock: unknown command ${cmd}`);
    await sleep(
      SLOW.has(cmd)
        ? SLOW_MS
        : cmd === "clones" || cmd === "doctor_repo"
          ? 90
          : cmd === "cleanup" || cmd === "repo_candidates"
            ? 400
            : cmd === "init_preview"
              ? 150
              : 25,
    );
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
