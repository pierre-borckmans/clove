// Realistic state for the browser build: three repos on a dev laptop.

import type { Check, CloneInfo, GoldenStatus, MergeCheck, RepoStatus, Step } from "@/lib/types";

const now = () => Math.floor(Date.now() / 1000);
const MB = 1024 * 1024;
const GB = 1024 * MB;

export type MockRepo = {
  status: Omit<RepoStatus, "clones" | "mounted">;
  clones: CloneInfo[];
  gc: string[];
  /** Last PR check per clone. */
  prs: MergeCheck[];
  /** Why the golden image is stale, or null when fresh. */
  stale: string | null;
  /** Config files that exist (by layer label). */
  layers: string[];
  refs: { name: string; kind: "local" | "remote" | "tag"; age: number; subject: string }[];
};

const pnpm: Step = {
  name: "pnpm",
  warm: "pnpm install --frozen-lockfile",
  setup: "pnpm install --offline --frozen-lockfile",
  watch: ["pnpm-lock.yaml"],
  wrap: false,
  env: {},
};
const cargo: Step = {
  name: "cargo",
  warm: "cargo build --workspace",
  setup: "cargo build --workspace",
  watch: ["Cargo.lock", "rust-toolchain.toml"],
  wrap: true,
  env: {},
};
const direnv: Step = {
  name: "direnv",
  warm: "direnv allow . && direnv exec . true",
  setup: "direnv allow .",
  watch: [".envrc", "flake.lock"],
  wrap: false,
  env: {},
};

function check(
  clone: string,
  branch: string | null,
  verdict: MergeCheck["verdict"],
  reason: string,
  pr: { number: number; state: "open" | "merged" | "closed" } | null,
  ownMb: number,
): MergeCheck {
  return {
    clone,
    branch,
    head: "8f3c2a1e9b7d",
    pr: pr && { ...pr, head_oid: "8f3c2a1e9b7d", title: "", url: `https://github.com/ada/mono/pull/${pr.number}` },
    verdict,
    reason,
    own_bytes: ownMb * MB,
  };
}

function golden(p: Partial<GoldenStatus> & Pick<GoldenStatus, "generation" | "commit">): GoldenStatus {
  const age = p.age_secs ?? 3 * 3600;
  return {
    base: "origin/main",
    created_at: now() - age,
    timings: [],
    steps: [],
    dirty: false,
    age_secs: age,
    behind: 0,
    image_bytes: GB,
    ...p,
  };
}

function clone(repo: string, p: Partial<CloneInfo> & Pick<CloneInfo, "name"> & { age: number }): CloneInfo {
  const { age, ...rest } = p;
  return {
    path: `/Users/ada/src/${repo}.clove/${p.name}/${repo}`,
    mounted: true,
    branch: null,
    head: "8f3c2a1e9b7d",
    dirty: false,
    ahead: 0,
    behind: 0,
    image_bytes: 64 * MB,
    created_at: now() - age,
    golden_generation: 14,
    timings: [
      { step: "fetch", ms: 412 },
      { step: "clone image", ms: 21 },
      { step: "mount", ms: 284 },
      { step: "checkout", ms: 171 },
    ],
    failed_steps: [],
    ...rest,
  };
}

export function seed(): Map<string, MockRepo> {
  const repos = new Map<string, MockRepo>();
  repos.set("/Users/ada/src/mono", {
    status: {
      name: "mono",
      root: "/Users/ada/src/mono",
      id: "mono-3fa81c02",
      state: "/Users/ada/.clove/repos/mono-3fa81c02",
      clone_root: "/Users/ada/src/mono.clove",
      base: "origin/main",
      backend: "asif",
      golden: golden({
        generation: 14,
        commit: "8f3c2a1e9b7d4c0fa1e2b3c4d5e6f708192a3b4c",
        age_secs: 3 * 3600 + 840,
        behind: 23,
        image_bytes: 6.8 * GB,
        steps: ["pnpm", "cargo", "direnv"],
        timings: [
          { step: "fetch", ms: 2140 },
          { step: "clone golden", ms: 34 },
          { step: "mount", ms: 402 },
          { step: "checkout", ms: 14_210 },
          { step: "warm pnpm", ms: 96_380 },
          { step: "warm cargo", ms: 212_900 },
          { step: "warm direnv", ms: 8_120 },
          { step: "index", ms: 1_830 },
          { step: "unmount", ms: 610 },
        ],
      }),
      steps: [pnpm, cargo, direnv],
      steps_detected: false,
    },
    clones: [
      clone("mono", {
        name: "feat-onboarding-checklist",
        branch: "feat/onboarding-checklist",
        dirty: true,
        ahead: 0,
        image_bytes: 48 * MB,
        age: 4 * 60,
        head: "8f3c2a1e9b7d",
      }),
      clone("mono", {
        name: "review-pr-1432",
        branch: null,
        head: "c41d07be22a9",
        image_bytes: 12 * MB,
        age: 22 * 60,
      }),
      clone("mono", {
        name: "fix-cache-stampede",
        branch: "fix/cache-stampede",
        ahead: 1,
        image_bytes: 96 * MB,
        age: 38 * 60,
        head: "a90e3f1c7d22",
      }),
      clone("mono", {
        name: "feat-billing-v2",
        branch: "feat/billing-v2",
        dirty: true,
        ahead: 4,
        behind: 2,
        image_bytes: 412 * MB,
        age: 2 * 3600 + 600,
        head: "5b6c7d8e9f01",
        timings: [
          { step: "fetch", ms: 388 },
          { step: "clone image", ms: 19 },
          { step: "mount", ms: 301 },
          { step: "checkout", ms: 240 },
          { step: "setup pnpm", ms: 6_820 },
        ],
      }),
      clone("mono", {
        name: "chore-bump-vite",
        branch: "chore/bump-vite",
        ahead: 2,
        behind: 11,
        image_bytes: 1.2 * GB,
        age: 26 * 3600,
        golden_generation: 13,
        head: "0d1e2f3a4b5c",
      }),
      clone("mono", {
        name: "release-2.14",
        branch: "release/2.14",
        ahead: 0,
        behind: 31,
        image_bytes: 160 * MB,
        age: 3 * 86_400,
        golden_generation: 12,
        failed_steps: ["cargo"],
        head: "77aa88bb99cc",
      }),
      clone("mono", {
        name: "exp-streaming-ssr",
        branch: "exp/streaming-ssr",
        mounted: false,
        dirty: null,
        ahead: null,
        behind: null,
        image_bytes: 2.3 * GB,
        age: 6 * 86_400,
        golden_generation: 11,
        head: "e1f2a3b4c5d6",
      }),
    ],
    gc: ["remove old golden gen-13", "remove empty mount point /Users/ada/src/mono.clove/spike-wasm"],
    prs: [
      check("chore-bump-vite", "chore/bump-vite", "prune", "PR #1428 merged", { number: 1428, state: "merged" }, 22),
      check("fix-cache-stampede", "fix/cache-stampede", "keep", "PR #1441 open", { number: 1441, state: "open" }, 96),
      check(
        "feat-billing-v2",
        "feat/billing-v2",
        "review",
        "PR #1402 merged, but 2 commits after it",
        { number: 1402, state: "merged" },
        412,
      ),
      check("release-2.14", "release/2.14", "keep", "no PR", null, 160),
    ],
    stale: "pnpm inputs changed in 2 commits",
    layers: ["you, this repo"],
    refs: [
      { name: "main", kind: "local", age: 3600, subject: "Merge pull request #1440 from feat/ledger-export" },
      { name: "origin/main", kind: "remote", age: 1800, subject: "fix(api): retry idempotent writes on 503" },
      { name: "feat/onboarding-checklist", kind: "local", age: 240, subject: "wip: checklist steps" },
      { name: "fix/cache-stampede", kind: "local", age: 2400, subject: "fix(cache): single-flight on miss" },
      { name: "feat/billing-v2", kind: "local", age: 9000, subject: "feat(billing): proration preview" },
      { name: "chore/bump-vite", kind: "local", age: 93_600, subject: "chore: vite 8" },
      { name: "release/2.14", kind: "local", age: 259_200, subject: "release: 2.14.3" },
      { name: "exp/streaming-ssr", kind: "local", age: 518_400, subject: "exp: stream the shell first" },
      { name: "origin/feat/ledger-export", kind: "remote", age: 7200, subject: "feat(ledger): CSV export" },
      { name: "origin/fix/flaky-e2e-login", kind: "remote", age: 14_400, subject: "test: wait for the session cookie" },
      { name: "origin/feat/audit-log-filters", kind: "remote", age: 30_000, subject: "feat(audit): filter by actor" },
      { name: "origin/chore/drop-node-20", kind: "remote", age: 86_000, subject: "chore: require node 22" },
      {
        name: "origin/perf/query-planner-hints",
        kind: "remote",
        age: 172_000,
        subject: "perf: hint the planner on hot paths",
      },
      { name: "origin/release/2.14", kind: "remote", age: 259_200, subject: "release: 2.14.3" },
      { name: "v2.14.3", kind: "tag", age: 259_200, subject: "release: 2.14.3" },
      { name: "v2.14.2", kind: "tag", age: 900_000, subject: "release: 2.14.2" },
    ],
  });
  repos.set("/Users/ada/src/clove", {
    status: {
      name: "clove",
      root: "/Users/ada/src/clove",
      id: "clove-91c2d7e4",
      state: "/Users/ada/.clove/repos/clove-91c2d7e4",
      clone_root: "/Users/ada/src/clove.clove",
      base: "origin/main",
      backend: "asif",
      golden: golden({
        generation: 3,
        commit: "3795a72c1d0e9f8a7b6c5d4e3f2a1b0c9d8e7f6a",
        age_secs: 50 * 60,
        behind: 0,
        image_bytes: 1.1 * GB,
        steps: ["cargo"],
        timings: [
          { step: "fetch", ms: 610 },
          { step: "clone golden", ms: 22 },
          { step: "mount", ms: 350 },
          { step: "checkout", ms: 420 },
          { step: "warm cargo", ms: 48_200 },
          { step: "index", ms: 90 },
          { step: "unmount", ms: 380 },
        ],
      }),
      steps: [cargo],
      steps_detected: true,
    },
    clones: [
      clone("clove", {
        name: "desktop",
        branch: "desktop",
        dirty: true,
        ahead: 3,
        image_bytes: 820 * MB,
        age: 40 * 60,
        golden_generation: 3,
      }),
      clone("clove", {
        name: "tui-polish",
        branch: "tui-polish",
        ahead: 1,
        image_bytes: 140 * MB,
        age: 5 * 3600,
        golden_generation: 3,
      }),
    ],
    gc: [],
    prs: [],
    stale: null,
    layers: ["team"],
    refs: [
      { name: "main", kind: "local", age: 3000, subject: "feat: clove core and CLI" },
      { name: "desktop", kind: "local", age: 2400, subject: "feat(desktop): tray popover" },
      { name: "tui-polish", kind: "local", age: 18_000, subject: "tui: keep selection on refresh" },
      { name: "origin/main", kind: "remote", age: 3000, subject: "feat: clove core and CLI" },
    ],
  });
  repos.set("/Users/ada/src/site", {
    status: {
      name: "site",
      root: "/Users/ada/src/site",
      id: "site-0b7e55a1",
      state: "/Users/ada/.clove/repos/site-0b7e55a1",
      clone_root: "/Users/ada/src/site.clove",
      base: "origin/main",
      backend: "asif",
      golden: golden({
        generation: 1,
        commit: "b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5",
        age_secs: 9 * 86_400,
        behind: 2,
        image_bytes: 740 * MB,
        steps: ["pnpm"],
        timings: [
          { step: "create image", ms: 820 },
          { step: "mount", ms: 330 },
          { step: "fetch", ms: 900 },
          { step: "checkout", ms: 1300 },
          { step: "warm pnpm", ms: 31_400 },
          { step: "index", ms: 120 },
          { step: "unmount", ms: 290 },
        ],
      }),
      steps: [pnpm],
      steps_detected: true,
    },
    clones: [],
    gc: [],
    prs: [],
    stale: null,
    layers: [],
    refs: [
      { name: "main", kind: "local", age: 800_000, subject: "content: pricing page copy" },
      { name: "origin/main", kind: "remote", age: 80_000, subject: "fix: og image size" },
    ],
  });
  return repos;
}

export const machineChecks: Check[] = [
  { name: "disk images", level: "ok", detail: "ASIF via diskutil (macOS 26+)", fix: null },
  { name: "state volume", level: "ok", detail: "/Users/ada/.clove is on APFS", fix: null },
  { name: "git", level: "ok", detail: "2.51.0", fix: null },
];

export const PNPM_OUTPUT = [
  "Lockfile is up to date, resolution step is skipped",
  "Progress: resolved 1832, reused 1829, downloaded 3, added 0",
  "Packages: +3 -1",
  "Progress: resolved 1832, reused 1829, downloaded 3, added 3, done",
  "dependencies:",
  "+ vite 8.3.8 (8.3.7 is installed)",
  "Done in 2.1s using pnpm v10.11.0",
];

export const CARGO_OUTPUT = [
  "   Compiling proc-macro2 v1.0.101",
  "   Compiling serde v1.0.228",
  "   Compiling tokio v1.48.0",
  "   Compiling api-core v0.1.0 (/Users/ada/src/mono/crates/api-core)",
  "   Compiling api v0.1.0 (/Users/ada/src/mono/crates/api)",
  "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 41.20s",
];
