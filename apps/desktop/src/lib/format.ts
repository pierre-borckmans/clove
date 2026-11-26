export function bytes(b: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = b;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  if (i === 0) return `${b} B`;
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export function ago(secs: number): string {
  if (secs < 45) return "just now";
  if (secs < 3600) return `${Math.max(1, Math.round(secs / 60))}m ago`;
  if (secs < 86_400) return `${Math.round(secs / 3600)}h ago`;
  if (secs < 86_400 * 30) return `${Math.round(secs / 86_400)}d ago`;
  return `${Math.round(secs / (86_400 * 30))}mo ago`;
}

export function since(unix: number): string {
  return ago(Math.max(0, Date.now() / 1000 - unix));
}

export function ms(v: number): string {
  if (v < 1000) return `${Math.round(v)}ms`;
  if (v < 60_000) return `${(v / 1000).toFixed(v < 10_000 ? 2 : 1)}s`;
  const m = Math.floor(v / 60_000);
  const s = Math.round((v % 60_000) / 1000);
  return `${m}m ${s.toString().padStart(2, "0")}s`;
}

export function shortSha(sha: string | null | undefined, n = 8): string {
  return sha ? sha.slice(0, n) : "";
}

/** `/Users/ada/src/mono` -> `~/src/mono` when it sits under a home dir. */
export function tildify(path: string): string {
  // macOS homes are under /Users, Linux ones under /home (root's is /root).
  return path.replace(/^(\/Users\/[^/]+|\/home\/[^/]+|\/root(?=\/|$))/, "~");
}
