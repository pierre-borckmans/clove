// Mirrors the serde shapes of clove-core and the desktop commands.

export type Timing = { step: string; ms: number };

export type RepoSummary = {
  id: string;
  name: string;
  root: string;
  clones: number;
  mounted: number;
  generation: number | null;
  behind: number | null;
  /** A new clone would rerun a step, or past the age/commit safety net. */
  stale: boolean;
  refreshing: boolean;
  /** Clones the last PR check found merged and clean. */
  prunable: number;
  error: string | null;
};

export type Step = {
  name: string;
  warm?: string | null;
  setup?: string | null;
  watch: string[];
  wrap: boolean;
  env: Record<string, string>;
};

export type GoldenStatus = {
  generation: number;
  commit: string;
  base: string;
  created_at: number;
  timings: Timing[];
  steps: string[];
  dirty: boolean;
  age_secs: number;
  behind: number | null;
  image_bytes: number;
};

export type RepoStatus = {
  name: string;
  root: string;
  id: string;
  state: string;
  clone_root: string;
  base: string;
  backend: "asif" | "sparse" | null;
  golden: GoldenStatus | null;
  clones: number;
  mounted: number;
  steps: Step[];
  steps_detected: boolean;
};

export type CloneInfo = {
  name: string;
  path: string;
  mounted: boolean;
  branch: string | null;
  head: string | null;
  dirty: boolean | null;
  ahead: number | null;
  behind: number | null;
  /** Allocated image size, mostly blocks shared with the golden image. */
  image_bytes: number;
  /** Bytes this clone does not share: what removing it frees. Missing in old mocks. */
  own_bytes?: number;
  /** Wall-clock time of `clove new`. */
  total_ms?: number;
  created_at: number;
  golden_generation: number;
  timings: Timing[];
  failed_steps: string[];
  /** Broken outside clove (worktree deleted, unregistered); cleanup removes it. */
  issue?: string;
  /** Removal asked while in use; it goes once nothing uses it. */
  removing_when_free?: boolean;
  /** A `clove update` (or a rebase or merge) stopped on conflicts. */
  operation?: "rebase" | "merge";
};

/** A plain git worktree clove didn't make (clove_core::OtherWorktree). */
export type OtherWorktree = {
  name: string;
  path: string;
  branch: string | null;
  head: string | null;
  locked: boolean;
  lock_reason: string | null;
  /** git's reason when it would prune the registration (folder gone). */
  prunable: string | null;
  missing: boolean;
  dirty: boolean | null;
  /** Detached only: commits on no branch, which removing it would orphan. */
  detached_commits: number | null;
  pr: Pr | null;
  /** Why clove won't remove or adopt it. */
  blocker: string | null;
  /** `git status --short` lines when dirty (at most a dozen) and their total. */
  changes?: string[];
  changes_total?: number;
  /** Only a lock and/or uncommitted changes block it: both can be overridden. */
  can_force?: boolean;
  /** Why it can't become a clone; uncommitted changes don't stop it (they move). */
  adopt_blocker?: string | null;
};

export type CheckLevel = "ok" | "warn" | "fail";
export type Check = { name: string; level: CheckLevel; detail: string; fix: string | null };

export type GitRef = { name: string; kind: "local" | "remote" | "tag"; date: number; subject: string };
export type Refs = { base: string; refs: GitRef[] };

/** A git repo clove doesn't manage yet (`clove repos --candidates`). */
export type RepoCandidate = { root: string; name: string; branch: string | null; active: number };

export type PreviewStep = { name: string; warm: string | null; setup: string | null; tool: string | null };

/** What `clove init` would do for a repo, computed without changing anything. */
export type InitPreview = {
  root: string;
  name: string;
  initialized: boolean;
  base: string;
  base_found: boolean;
  steps: PreviewStep[];
  steps_detected: boolean;
  nested_skipped: string[];
  copy: string[];
  exec: "auto" | "direnv" | "mise" | "none";
  image_size: string;
  clone_root: string;
  state: string;
  missing_tools: string[];
  machine: Check[];
};

export type Discovered = {
  root: string;
  name: string;
  initialized: boolean;
  base: string;
  steps: string[];
};

export type AppId =
  | "terminal"
  | "ghostty"
  | "iterm"
  | "warp"
  | "kitty"
  | "alacritty"
  | "wezterm"
  | "vscode"
  | "cursor"
  | "zed"
  | "finder";

export type OpRequest =
  | { kind: "init"; root: string }
  | { kind: "refresh"; root: string }
  | {
      kind: "new";
      root: string;
      branch?: string | null;
      base?: string | null;
      name?: string | null;
      no_setup?: boolean;
      detach?: boolean;
    }
  | { kind: "remove"; root: string; name: string; force?: boolean; delete_branch?: boolean }
  | { kind: "remove_worktree"; root: string; path: string; delete_branch?: boolean; force?: boolean; unlock?: boolean }
  | { kind: "adopt"; root: string; path: string; name?: string | null }
  | { kind: "deinit"; root: string }
  | { kind: "update"; root: string; name: string; step?: "run" | "continue" | "abort"; autostash?: boolean };

/** What `clove update` did (clove_core::update::Update). */
export type UpdateResult =
  | { result: "up_to_date"; base: string }
  | {
      result: "updated";
      base: string;
      strategy: "rebase" | "merge";
      commits: number | null;
      hint: string | null;
      failed_steps: string[];
    }
  | { result: "conflict"; base: string; strategy: "rebase" | "merge"; files: string[] };

export type CloveEvent =
  | { event: "start"; step: string }
  | { event: "done"; step: string; took: number }
  | { event: "skip"; step: string; reason: string }
  | { event: "output"; step: string; line: string }
  | { event: "warn"; message: string }
  | { event: "info"; message: string };

export type OpStarted = { id: string; at: number; op: OpRequest };
export type OpEvent = { id: string; at: number; event: CloveEvent };
export type OpFinished = { id: string; at: number; ok: boolean; error: string | null; result: unknown };

export type Freshness = {
  age_secs: number;
  behind: number | null;
  stale: boolean;
  changed_steps: string[];
  reason: string | null;
  refreshing: number | null;
};

export type Config = {
  base: string;
  clone_root: string;
  image_size: string;
  fetch: boolean;
  fetch_interval: number;
  exec: "auto" | "direnv" | "mise" | "none";
  env: Record<string, string>;
  steps: Step[];
  steps_detected: boolean;
  /** Configured copy pathspecs, or null when env-like ignored files are detected. */
  copy: string[] | null;
  refresh: { auto: boolean; max_age_secs: number; max_behind: number };
  prune: { auto: boolean };
  /** resolver: the command offered on conflicts; null when off. */
  update: { strategy: "rebase" | "merge"; resolver: string | null };
  sources: string[];
};

export type ConfigLayer = { label: string; path: string; exists: boolean };

export type RepoConfig = {
  config: Config;
  layers: ConfigLayer[];
  copy: string[];
  freshness: Freshness | null;
  state_home: string;
  fields: FieldState[];
};

/** A config file, lowest precedence first: all repos, team, you for this repo. */
export type Layer = "all" | "team" | "you";
export const LAYERS: Layer[] = ["all", "team", "you"];

/** clove_core::config::Kind: how a setting is edited. */
export type FieldKind =
  | { type: "text" | "bool" | "seconds" | "duration" | "int" | "size" | "list" | "map" }
  | { type: "choice"; options: string[] };

/** clove_core::settings::FieldState: one setting across the config files. */
export type FieldState = {
  key: string;
  label: string;
  section: string;
  kind: FieldKind;
  help: string;
  default: { value: string } | "resolved";
  /** Per layer, in LAYERS order; null where that file doesn't set it. */
  layers: (string | null)[];
  effective: string;
  /** null: the default applies. */
  source: Layer | null;
};

export type PrState = "open" | "merged" | "closed";
export type Pr = { number: number; state: PrState; head_oid: string; title: string; url: string };
export type Verdict = "prune" | "review" | "keep";
export type MergeCheck = {
  clone: string;
  branch: string | null;
  head: string | null;
  pr: Pr | null;
  verdict: Verdict;
  reason: string;
  own_bytes: number;
};

export type WorktreeCheck = {
  name: string;
  path: string;
  branch: string | null;
  head: string | null;
  pr: Pr | null;
  verdict: Verdict;
  reason: string;
};

export type Cleanup = {
  leftovers: string[];
  checks: MergeCheck[];
  worktrees: WorktreeCheck[];
  warning: string | null;
  removed: string[];
  removed_worktrees: string[];
  kept: string[];
};

/** Whether coding agents know clove (`clove agents`); null when none is installed. */
export type AgentsState = "installed" | "outdated" | "missing" | null;

/** One removal from `clove log`. */
export type Removed = {
  repo: string;
  root: string;
  at: number;
  kind: "clone" | "worktree";
  name: string;
  path: string;
  branch: string | null;
  head: string | null;
  by: "hand" | "prune" | "auto-prune" | "claude-hook";
  reason: string | null;
  branch_deleted: boolean;
  /** The command that brings the branch back, when it was deleted. */
  restore: string | null;
};

export type ShellStatus = {
  shell: "zsh" | "bash" | "fish" | null;
  installed: boolean;
  rc: string | null;
  line: string | null;
};

/** A clone's changes against the merge-base with its base (`clove_core::CloneDiff`). */
export type CloneDiff = {
  base: string;
  merge_base: string;
  patch: string;
  truncated: boolean;
  skipped: string[];
  attrs: FileAttrs[];
  /** Innermost first: a file belongs to the longest `dir` prefix. */
  packages: Package[];
};

/** A directory with a package manifest; `dir` is "" for the repo root. */
export type Package = { dir: string; name: string; manifest: string };

/** What `.gitattributes` says about a changed file, when it matters for display. */
export type FileAttrs = {
  path: string;
  /** `binary` or `-diff`. */
  binary: boolean;
  /** `linguist-generated` or `linguist-vendored`. */
  generated: boolean;
  /** A `filter` (LFS) or a `diff=<driver>` textconv: no whole-file context. */
  converted: boolean;
};
