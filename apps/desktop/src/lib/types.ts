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
  image_bytes: number;
  created_at: number;
  golden_generation: number;
  timings: Timing[];
  failed_steps: string[];
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
  | { event: "warn"; message: string };

export type OpStarted = { id: string; at: number; op: OpRequest };
export type OpEvent = { id: string; at: number; event: CloveEvent };
export type OpFinished = { id: string; at: number; ok: boolean; error: string | null; result: unknown };
