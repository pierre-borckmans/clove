import { invoke, isTauri } from "./bridge";
import type {
  AppId,
  Check,
  Cleanup,
  CloneInfo,
  Discovered,
  InitPreview,
  Layer,
  MergeCheck,
  OpRequest,
  Refs,
  RepoCandidate,
  RepoConfig,
  RepoStatus,
  RepoSummary,
  ShellStatus,
  Step,
} from "./types";

/**
 * clove-core leaves a step's defaults out of its JSON (an empty `watch` or
 * `env`, `wrap = true`) so `.clove.toml` stays short. Put them back here, once,
 * so screens can trust the type.
 */
export function normStep(s: Partial<Step> & { name: string }): Step {
  return { ...s, watch: s.watch ?? [], env: s.env ?? {}, wrap: s.wrap ?? true };
}

export const api = {
  repos: () => invoke<RepoSummary[]>("repos"),
  status: (root: string) =>
    invoke<RepoStatus>("repo_status", { root }).then((s) => ({ ...s, steps: s.steps.map(normStep) })),
  clones: (root: string, status: boolean) => invoke<CloneInfo[]>("clones", { root, status }),
  refs: (root: string) => invoke<Refs>("refs", { root }),
  doctorMachine: () => invoke<Check[]>("doctor_machine"),
  doctorRepo: (root: string) => invoke<Check[]>("doctor_repo", { root }),
  gc: (root: string, apply: boolean) => invoke<string[]>("gc", { root, apply }),
  mount: (root: string, name: string) => invoke<string>("mount_clone", { root, name }),
  unmount: (root: string, name: string, force = false) => invoke<void>("unmount_clone", { root, name, force }),
  discover: (dir: string) => invoke<Discovered>("discover", { dir }),
  repoCandidates: () => invoke<RepoCandidate[]>("repo_candidates"),
  initPreview: (dir: string) => invoke<InitPreview>("init_preview", { dir }),
  installedApps: () => invoke<AppId[]>("installed_apps"),
  openIn: (path: string, app: AppId) => invoke<void>("open_in", { path, app }),
  copy: (text: string) => invoke<void>("copy_text", { text }),
  repoConfig: (root: string) =>
    invoke<RepoConfig>("repo_config", { root }).then((r) => ({
      ...r,
      config: { ...r.config, steps: r.config.steps.map(normStep) },
    })),
  editConfig: (path: string) => invoke<void>("edit_config", { path }),
  /** Sets a setting in one config file; a null value removes it. */
  configSet: (root: string, layer: Layer, key: string, value: string | null) =>
    invoke<string>("config_set", { root, layer, key, value }),
  serviceStatus: () => invoke<boolean>("service_status"),
  serviceSet: (install: boolean) => invoke<boolean>("service_set", { install }),
  prs: (root: string) => invoke<MergeCheck[]>("prs", { root }),
  checkPrs: (root: string) => invoke<MergeCheck[]>("check_prs", { root }),
  branchCandidates: (root: string) => invoke<{ name: string; remote: boolean }[]>("branch_candidates", { root }),
  cleanup: (root: string, apply: boolean) => invoke<Cleanup>("cleanup", { root, apply }),
  shellStatus: () => invoke<ShellStatus>("shell_status"),
  shellInstall: () => invoke<ShellStatus>("shell_install"),
  startOp: (op: OpRequest) => invoke<string>("start_op", { op }),
  openMain: (route?: string, action?: string) => invoke<void>("open_main", { route, action }),
  hidePopover: () => invoke<void>("hide_popover"),
  quit: () => invoke<void>("quit"),
};

/** Folder picker. Returns null when cancelled. */
export async function pickFolder(): Promise<string | null> {
  if (!isTauri) {
    const { mockPickFolder } = await import("@/mock/backend");
    return mockPickFolder();
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const res = await open({ directory: true, multiple: false, title: "Choose a git repository" });
  return typeof res === "string" ? res : null;
}
