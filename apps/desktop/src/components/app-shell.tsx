import { Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { useEffect } from "react";
import { Toaster } from "@/components/ui/sonner";
import { useHotkey } from "@/hooks/use-hotkey";
import { listen } from "@/lib/bridge";
import { useRepos } from "@/lib/queries";
import { getUi, setUi } from "@/lib/ui";
import { AddRepoDialog } from "./add-repo-dialog";
import { CommandPalette } from "./command-palette";
import { NewCloneDialog } from "./new-clone-dialog";
import { OpsPanel } from "./ops-panel";
import { RemoveCloneDialog } from "./remove-clone-dialog";
import { Sidebar } from "./sidebar";

export function AppShell() {
  const navigate = useNavigate();
  const { data: repos } = useRepos();
  const params = useParams({ strict: false }) as { repoId?: string };
  const current = repos?.find((r) => r.id === params.repoId);

  useHotkey({ key: "k", meta: true }, () => setUi({ palette: !getUi().palette }));
  useHotkey({ key: "n", meta: true }, () =>
    setUi({ newClone: { root: current?.root ?? repos?.[0]?.root ?? null }, palette: false }),
  );
  useHotkey({ key: "o", meta: true }, () => setUi({ addRepo: true, palette: false }));
  useHotkey({ key: ",", meta: true }, () => {
    setUi({ palette: false });
    void navigate({ to: "/settings", search: current ? { repo: current.id } : {} });
  });
  // The webview would reload on ⌘R; refetch instead.
  useHotkey({ key: "r", meta: true }, () => window.dispatchEvent(new Event("clove:refetch")));
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
      const n = Number(e.key);
      if (n >= 1 && n <= 9 && repos?.[n - 1]) {
        e.preventDefault();
        void navigate({ to: "/repo/$repoId", params: { repoId: repos[n - 1].id } });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repos, navigate]);

  // The menu-bar popover asks the main window to show a repo or open a dialog.
  useEffect(() => {
    const un = [
      listen<string>("navigate", (to) => void navigate({ to })),
      listen<string>("action", (a) => {
        if (a.startsWith("new-clone")) {
          const root = a.split(":").slice(1).join(":") || null;
          setUi({ newClone: { root } });
        }
        if (a === "add-repo") setUi({ addRepo: true });
      }),
    ];
    return () => {
      for (const p of un) void p.then((f) => f());
    };
  }, [navigate]);

  return (
    <div className="flex h-full">
      <Sidebar />
      <main className="relative my-2 mr-2 flex min-w-0 flex-1 flex-col overflow-hidden rounded-[10px] bg-background shadow-[var(--pane-shadow)]">
        <Outlet />
        <OpsPanel />
      </main>
      <CommandPalette />
      <NewCloneDialog />
      <AddRepoDialog />
      <RemoveCloneDialog />
      <Toaster position="bottom-right" offset={{ right: 16, bottom: 16 }} />
    </div>
  );
}

/** A pane header that doubles as the window's drag region. */
export function PaneHeader({ children }: { children: React.ReactNode }) {
  return (
    <header data-tauri-drag-region className="flex h-[52px] shrink-0 items-center gap-3 border-b px-5">
      {children}
    </header>
  );
}
