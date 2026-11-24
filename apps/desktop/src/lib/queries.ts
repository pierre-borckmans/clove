import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api } from "./api";
import { APP_LABELS } from "./prefs";
import type { AppId } from "./types";

export const keys = {
  repos: ["repos"] as const,
  status: (root: string) => ["status", root] as const,
  clones: (root: string) => ["clones", root] as const,
  refs: (root: string) => ["refs", root] as const,
  doctorMachine: ["doctor", "machine"] as const,
  doctorRepo: (root: string) => ["doctor", "repo", root] as const,
  gc: (root: string) => ["gc", root] as const,
  apps: ["apps"] as const,
};

export function useRepos() {
  return useQuery({ queryKey: keys.repos, queryFn: api.repos, refetchInterval: 5000 });
}

/** The repo for a route id, from the repos list. */
export function useRepo(id: string | undefined) {
  const q = useRepos();
  return { ...q, repo: q.data?.find((r) => r.id === id) };
}

export function useStatus(root: string | undefined) {
  return useQuery({
    queryKey: keys.status(root ?? ""),
    queryFn: () => api.status(root as string),
    enabled: !!root,
    refetchInterval: 5000,
  });
}

export function useClones(root: string | undefined) {
  return useQuery({
    queryKey: keys.clones(root ?? ""),
    queryFn: () => api.clones(root as string, true),
    enabled: !!root,
    refetchInterval: 4000,
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
