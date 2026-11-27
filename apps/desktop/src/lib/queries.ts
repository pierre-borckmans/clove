import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { api } from "./api";
import { listen } from "./bridge";
import { APP_LABELS } from "./prefs";
import type { AppId, Layer, Removed, RepoRef } from "./types";
import { setUi } from "./ui";

export const keys = {
  repos: ["repos"] as const,
  /** Under `repos`, so whatever refreshes the repos refreshes this too. */
  repoList: ["repos", "list"] as const,
  status: (root: string) => ["status", root] as const,
  clones: (root: string) => ["clones", root] as const,
  worktrees: (root: string) => ["worktrees", root] as const,
  refs: (root: string) => ["refs", root] as const,
  doctorMachine: ["doctor", "machine"] as const,
  doctorRepo: (root: string) => ["doctor", "repo", root] as const,
  gc: (root: string) => ["gc", root] as const,
  cleanup: (root: string) => ["cleanup", root] as const,
  diff: (root: string, name: string) => ["diff", root, name] as const,
  config: (root: string) => ["config", root] as const,
  prs: (root: string) => ["prs", root] as const,
  service: ["service"] as const,
  agents: ["agents"] as const,
  history: ["history"] as const,
  stats: ["stats"] as const,
  shell: ["shell"] as const,
  apps: ["apps"] as const,
  repoCandidates: ["repo-candidates"] as const,
  initPreview: (dir: string) => ["init-preview", dir] as const,
  upgrade: ["upgrade"] as const,
};

/**
 * Background poll. The backend's filesystem watcher sends `repos-changed` for
 * clones, golden images, fetches and PR checks (see `useRepoChanges`); this
 * catches the rest, like a clone's working tree getting dirty.
 */
export const POLL_MS = 15_000;
const PENDING_POLL_MS = 500;

/** Refetches repo data as soon as the backend sees it change on disk. */
export function useRepoChanges() {
  const qc = useQueryClient();
  useEffect(() => {
    const un = listen<{ treesOnly?: boolean } | null>("repos-changed", (change) => {
      // Edits inside working trees change clone and worktree status only.
      const only = change?.treesOnly ? ["clones", "worktrees", "diff"] : null;
      void qc.invalidateQueries({
        predicate: (q) => {
          const k = String(q.queryKey[0]);
          if (only) return only.includes(k);
          return !["prs-check", "doctor", "apps", "shell", "repo-candidates", "init-preview", "upgrade"].includes(k);
        },
      });
    });
    return () => void un.then((f) => f());
  }, [qc]);
}

/** Git repos clove could manage, scanned from the usual code folders. */
export function useRepoCandidates(enabled: boolean) {
  return useQuery({ queryKey: keys.repoCandidates, queryFn: api.repoCandidates, enabled, staleTime: 0 });
}

/** What `init` would do for `dir`; null skips. */
export function useInitPreview(dir: string | null) {
  return useQuery({
    queryKey: keys.initPreview(dir ?? ""),
    queryFn: () => api.initPreview(dir as string),
    enabled: !!dir,
    staleTime: 0,
    retry: false,
  });
}

export function useRepos() {
  return useQuery({ queryKey: keys.repos, queryFn: api.repos, refetchInterval: POLL_MS });
}

/**
 * Known repos without their status. It answers in a millisecond where
 * `useRepos` runs git in every repo, so pages and the sidebar draw from it first.
 */
export function useRepoList() {
  return useQuery({ queryKey: keys.repoList, queryFn: api.repoList, refetchInterval: POLL_MS });
}

/** The repo for a route id: from the repos list, or the quick list until that loads. */
export function useRepo(id: string | undefined) {
  const q = useRepos();
  const list = useRepoList();
  const repo: RepoRef | undefined = q.data?.find((r) => r.id === id) ?? list.data?.find((r) => r.id === id);
  return { ...q, isLoading: q.isLoading || list.isLoading, repo };
}

export function useStatus(root: string | undefined) {
  return useQuery({
    queryKey: keys.status(root ?? ""),
    queryFn: () => api.status(root as string),
    enabled: !!root,
    refetchInterval: POLL_MS,
  });
}

export function useClones(root: string | undefined) {
  return useQuery({
    queryKey: keys.clones(root ?? ""),
    queryFn: () => api.clones(root as string, true),
    enabled: !!root,
    // Statuses shown from the saved copy: pick up git's answers soon.
    refetchInterval: (q) => (q.state.data?.some((c) => c.status_pending) ? PENDING_POLL_MS : POLL_MS),
  });
}

/** The repo's plain git worktrees (ones clove didn't make). */
export function useWorktrees(root: string | undefined) {
  return useQuery({
    queryKey: keys.worktrees(root ?? ""),
    queryFn: () => api.worktrees(root as string, true),
    enabled: !!root,
    refetchInterval: POLL_MS,
  });
}

export function useRefs(root: string | undefined, enabled = true) {
  return useQuery({
    queryKey: keys.refs(root ?? ""),
    queryFn: () => api.refs(root as string),
    enabled: !!root && enabled,
    staleTime: 10_000,
  });
}

/** Existing branches worth offering for a new clone. */
export function useBranchCandidates(root: string | undefined) {
  return useQuery({
    queryKey: ["branch-candidates", root ?? ""],
    queryFn: () => api.branchCandidates(root as string),
    enabled: !!root,
    staleTime: 10_000,
  });
}

export function useRepoConfig(root: string | undefined) {
  return useQuery({
    queryKey: keys.config(root ?? ""),
    queryFn: () => api.repoConfig(root as string),
    enabled: !!root,
    refetchInterval: POLL_MS,
  });
}

/** The last PR check per clone, by clone name. */
export function usePrs(root: string | undefined) {
  return useQuery({
    queryKey: keys.prs(root ?? ""),
    queryFn: () => api.prs(root as string),
    enabled: !!root,
    refetchInterval: 15_000,
    select: (list) => new Map(list.map((m) => [m.clone, m])),
  });
}

/**
 * While a repo is on screen, asks GitHub about its clones' PRs every few
 * minutes, so merges made by other tools and agents show up here without the
 * background service.
 */
export function usePrCheck(root: string | undefined) {
  const qc = useQueryClient();
  return useQuery({
    queryKey: ["prs-check", root ?? ""],
    queryFn: async () => {
      const checks = await api.checkPrs(root as string);
      qc.setQueryData(keys.prs(root as string), checks);
      void qc.invalidateQueries({ queryKey: keys.repos });
      return checks.length;
    },
    enabled: !!root,
    refetchInterval: 5 * 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
}

/**
 * The latest release and the versions in play. Polls every 10 minutes and on
 * focus, which also notices a CLI or app upgraded by hand; GitHub itself is
 * asked at most hourly (clove-core's cache, shared with the CLI and TUI).
 */
export function useUpgrade() {
  return useQuery({
    queryKey: keys.upgrade,
    queryFn: () => api.checkUpgrade(false),
    refetchInterval: 10 * 60_000,
    staleTime: 60_000,
    retry: false,
  });
}

/** A forced check; says "up to date" or opens the update dialog. */
export function useCheckUpgradeNow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.checkUpgrade(true),
    onSuccess: (s) => {
      qc.setQueryData(keys.upgrade, s);
      if (s.error) toast.error("Could not check for updates", { description: s.error });
      else if (s.available || s.restart_to) setUi({ upgrade: true });
      else toast.success(`clove ${s.current} is up to date`);
    },
    onError: (e) => toast.error("Could not check for updates", { description: String(e) }),
  });
}

export function useService() {
  return useQuery({ queryKey: keys.service, queryFn: api.serviceStatus, refetchInterval: 15_000 });
}

export function useServiceToggle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (install: boolean) => api.serviceSet(install),
    onSuccess: (on) =>
      toast.success(on ? "Service installed" : "Service removed", {
        description: on
          ? "Refreshes stale golden images when you pull, remounts clones at login"
          : "Golden images refresh only when a new clone finds them stale",
      }),
    onError: (e) => toast.error("Could not change the service", { description: String(e) }),
    onSettled: () => qc.invalidateQueries({ queryKey: keys.service }),
  });
}

export function useAgents() {
  return useQuery({ queryKey: keys.agents, queryFn: api.agentsStatus, staleTime: 30_000 });
}

export function useAgentsToggle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (install: boolean) => api.agentsSet(install),
    onSuccess: (state) =>
      toast.success(state === "installed" ? "Coding agents now know clove" : "clove taken out of agent instructions", {
        description:
          state === "installed"
            ? "Claude Code and Codex use clove for worktrees. Restart running sessions to pick it up."
            : "They go back to git worktree add",
      }),
    onError: (e) => toast.error("Could not change agent instructions", { description: String(e) }),
    onSettled: () => qc.invalidateQueries({ queryKey: keys.agents }),
  });
}

/** What clove removed in every repo, newest first. */
export function useHistory() {
  return useQuery({ queryKey: keys.history, queryFn: () => api.history(50), refetchInterval: POLL_MS });
}

/**
 * Announces clones the service auto-pruned while the app is open. What
 * happened before shows on the Cleanup page.
 */
export function useAutoPruneToasts() {
  const { data } = useHistory();
  const seen = useRef<number | null>(null);
  useEffect(() => {
    if (!data) return;
    const newest = data[0]?.at ?? 0;
    if (seen.current !== null) {
      const since = seen.current;
      for (const e of data.filter((e: Removed) => e.at > since && e.by === "auto-prune")) {
        toast.info(`Auto-pruned ${e.name} in ${e.repo}`, {
          description: [e.reason, e.restore && `branch back: ${e.restore}`].filter(Boolean).join(" · "),
        });
      }
    }
    seen.current = Math.max(seen.current ?? 0, newest);
  }, [data]);
}

export function useShell() {
  return useQuery({ queryKey: keys.shell, queryFn: api.shellStatus, staleTime: 30_000 });
}

export function useShellInstall() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.shellInstall,
    onSuccess: (s) => {
      qc.setQueryData(keys.shell, s);
      toast.success(`Added to ${s.rc?.replace(/^\/Users\/[^/]+/, "~") ?? "your shell rc"}`, {
        description: "Open a new terminal to use it",
      });
    },
    onError: (e) => toast.error("Could not set up the shell integration", { description: String(e) }),
  });
}

/** Writes one setting to one config file (null removes it), then refetches the config. */
export function useConfigSet(root: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { layer: Layer; key: string; value: string | null; layerLabel: string }) =>
      api.configSet(root, v.layer, v.key, v.value),
    onSuccess: (msg, v) => toast.success(msg, { description: `Saved to ${v.layerLabel}` }),
    onError: (e, v) => toast.error(`Could not save ${v.key}`, { description: String(e) }),
    onSettled: () => qc.invalidateQueries({ queryKey: keys.config(root) }),
  });
}

export async function editConfig(path: string) {
  try {
    await api.editConfig(path);
  } catch (e) {
    toast.error("Could not open the config file", { description: String(e) });
  }
}

export function useInstalledApps() {
  return useQuery({ queryKey: keys.apps, queryFn: api.installedApps, staleTime: 60_000 });
}

export function useMountToggle(root: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, mounted }: { name: string; mounted: boolean }) =>
      mounted ? api.unmount(root, name) : api.mount(root, name).then(() => undefined),
    onSuccess: (_, v) => toast.success(v.mounted ? `Unmounted ${v.name}` : `Mounted ${v.name}`),
    onError: (e, v) =>
      toast.error(v.mounted ? `Could not unmount ${v.name}` : `Could not mount ${v.name}`, {
        description: String(e),
      }),
    onSettled: () => qc.invalidateQueries(),
  });
}

export async function openIn(path: string, app: AppId) {
  try {
    await api.openIn(path, app);
  } catch (e) {
    toast.error(`Could not open in ${APP_LABELS[app]}`, { description: String(e) });
  }
}

export async function copyText(text: string, label: string) {
  try {
    await api.copy(text);
    toast(label, { description: text });
  } catch (e) {
    toast.error("Could not copy", { description: String(e) });
  }
}

export const copyPath = (path: string) => copyText(path, "Path copied");

/** A clone's patch; refetched when its tree changes, like the clone list. */
export function useCloneDiff(root: string | undefined, name: string) {
  return useQuery({
    queryKey: keys.diff(root ?? "", name),
    queryFn: () => api.cloneDiff(root as string, name),
    enabled: !!root,
    refetchInterval: POLL_MS,
    retry: false,
  });
}

/** Clone timings per repo (`clove stats --all-repos`). */
export function useStats() {
  return useQuery({ queryKey: keys.stats, queryFn: () => api.stats(12), refetchInterval: POLL_MS });
}
