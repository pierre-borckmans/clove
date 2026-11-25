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
};

export type CheckLevel = "ok" | "warn" | "fail";
export type Check = { name: string; level: CheckLevel; detail: string; fix: string | null };

export type GitRef = { name: string; kind: "local" | "remote" | "tag"; date: number; subject: string };
export type Refs = { base: string; refs: GitRef[] };

export type Discovered = {
  root: string;
  name: string;
  initialized: boolean;
  base: string;
  steps: string[];
};

export type AppId = "terminal" | "ghostty" | "iterm" | "warp" | "vscode" | "cursor" | "zed" | "finder";

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
  | { kind: "deinit"; root: string };

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
  sources: string[];
};

export type ConfigLayer = { label: string; path: string; exists: boolean };

export type RepoConfig = {
  config: Config;
  layers: ConfigLayer[];
  copy: string[];
  freshness: Freshness | null;
  state_home: string;
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

export type Cleanup = {
  leftovers: string[];
  checks: MergeCheck[];
  warning: string | null;
  removed: string[];
  kept: string[];
};

export type ShellStatus = {
  shell: "zsh" | "bash" | "fish" | null;
  installed: boolean;
  rc: string | null;
  line: string | null;
};
