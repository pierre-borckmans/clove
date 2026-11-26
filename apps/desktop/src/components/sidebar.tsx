import { Link, useParams, useRouterState } from "@tanstack/react-router";
import { Activity, BrushCleaning, History, Loader, Plus, Search, Settings, Settings2, Stethoscope } from "lucide-react";
import type { ReactNode } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { isTauri } from "@/lib/bridge";
import { useOps } from "@/lib/ops";
import { isMac, MOD } from "@/lib/platform";
import { APP_LABELS, EDITORS, setPrefs, TERMINALS, usePrefs } from "@/lib/prefs";
import { useHistory, useInstalledApps, useRepos, useService } from "@/lib/queries";
import type { RepoSummary } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";
import { Shortcut } from "./shortcut";

export function Sidebar() {
  const { data: repos } = useRepos();
  const ops = useOps();
  const params = useParams({ strict: false }) as { repoId?: string };
  const running = ops.filter((o) => o.status === "running");
  const prunable = repos?.reduce((a, r) => a + r.prunable, 0) ?? 0;
  const { data: history } = useHistory();
  const autoPruned = history?.filter((e) => e.by === "auto-prune" && Date.now() / 1000 - e.at < 86_400).length ?? 0;

  return (
    <aside className="flex h-full w-[236px] shrink-0 flex-col bg-sidebar">
      <div data-tauri-drag-region className="flex h-[52px] shrink-0 items-center gap-2 pl-[19px]">
        {/* Stand-ins for the traffic lights when the UI runs in a browser. */}
        {!isTauri &&
          isMac &&
          ["#ff5f57", "#febc2e", "#28c840"].map((c) => (
            <span
              key={c}
              className="size-3 rounded-full shadow-[inset_0_0_0_0.5px_rgb(0_0_0/0.12)]"
              style={{ background: c }}
            />
          ))}
      </div>
      <div className="px-3">
        <button
          type="button"
          onClick={() => setUi({ palette: true })}
          className="group flex h-8 w-full items-center gap-2 rounded-md border border-sidebar-line bg-background/50 px-2.5 text-[13px] text-muted-foreground shadow-[0_1px_0_0_var(--sidebar-line)] transition-colors hover:bg-background/80 hover:text-foreground"
        >
          <Search className="size-3.5" />
          <span className="flex-1 text-left">Search</span>
          <Shortcut keys="mod+k" />
        </button>
      </div>

      <nav className="mt-5 flex min-h-0 flex-1 flex-col overflow-y-auto px-2">
        <div className="group/section flex items-center justify-between px-2 pb-1">
          <span className="text-[11.5px] font-medium text-muted-foreground">Repositories</span>
          <button
            type="button"
            aria-label="Add repository"
            onClick={() => setUi({ addRepo: true })}
            className="rounded p-0.5 text-muted-foreground opacity-0 transition-opacity group-hover/section:opacity-100 hover:bg-foreground/5 hover:text-foreground"
          >
            <Plus className="size-3.5" />
          </button>
        </div>
        <div className="flex flex-col gap-px">
          {repos?.map((r, i) => {
            const busy = running.some((o) => o.req.root === r.root);
            const active = params.repoId === r.id;
            return (
              <Link
                key={r.id}
                to="/repo/$repoId"
                params={{ repoId: r.id }}
                className={cn(
                  "group flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] transition-colors",
                  active
                    ? "bg-foreground/[0.075] font-medium text-foreground"
                    : "text-foreground/80 hover:bg-foreground/[0.045] hover:text-foreground",
                )}
              >
                <RepoMark name={r.name} active={active} />
                <span className="min-w-0 flex-1 truncate">{r.name}</span>
                {busy || r.refreshing ? (
                  <Loader className="size-3 animate-spin text-bottle [animation-duration:1.4s]" />
                ) : (
                  <RepoState repo={r} />
                )}
                {!busy && i < 9 && (
                  <span className="hidden font-mono text-[11px] text-faint group-hover:inline">
                    {MOD}
                    {i + 1}
                  </span>
                )}
              </Link>
            );
          })}
          <button
            type="button"
            onClick={() => setUi({ addRepo: true })}
            className="flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] text-muted-foreground transition-colors hover:bg-foreground/[0.045] hover:text-foreground"
          >
            <span className="flex size-[18px] items-center justify-center rounded-[5px] border border-dashed border-foreground/20">
              <Plus className="size-3" />
            </span>
            Add repository…
          </button>
        </div>
      </nav>

      <div className="flex flex-col gap-px px-2 pb-3">
        <NavItem
          to="/settings"
          search={params.repoId ? { repo: params.repoId } : {}}
          icon={<Settings />}
          right={<ServiceState />}
        >
          Settings
        </NavItem>
        <NavItem to="/doctor" icon={<Stethoscope />}>
          Doctor
        </NavItem>
        <NavItem
          to="/cleanup"
          icon={<BrushCleaning />}
          right={
            prunable > 0 ? (
              <span className="rounded-full bg-sage-soft px-1.5 font-mono text-[10.5px] leading-[18px] text-sage">
                {prunable} merged
              </span>
            ) : null
          }
        >
          Cleanup
        </NavItem>
        <NavItem
          to="/history"
          icon={<History />}
          right={
            autoPruned > 0 ? (
              <span
                title="Auto-pruned in the last day"
                className="rounded-full bg-amber/10 px-1.5 font-mono text-[10.5px] leading-[18px] text-amber"
              >
                {autoPruned} pruned
              </span>
            ) : null
          }
        >
          History
        </NavItem>
        <button
          type="button"
          onClick={() => setUi({ activity: true })}
          className="flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] text-foreground/80 transition-colors hover:bg-foreground/[0.045] hover:text-foreground [&_svg]:size-4 [&_svg]:text-muted-foreground"
        >
          <Activity />
          <span className="flex-1 text-left">Activity</span>
          {running.length > 0 && (
            <span className="rounded-full bg-bottle-soft px-1.5 font-mono text-[10.5px] leading-[18px] text-bottle">
              {running.length}
            </span>
          )}
        </button>
        <PrefsMenu />
      </div>
    </aside>
  );
}

/** Clone count and a dot for the golden image: sage when fresh, amber when stale; "set up" before init. */
function RepoState({ repo: r }: { repo: RepoSummary }) {
  if (r.error) return <span className="size-1.5 rounded-full bg-destructive" title={r.error} />;
  if (r.generation === null) return <span className="text-[11px] text-amber group-hover:hidden">set up</span>;
  return (
    <span
      className="flex items-center gap-1.5 group-hover:hidden"
      title={r.stale ? "Golden image is stale: a new clone would redo work" : "Golden image is fresh"}
    >
      <span className="font-mono text-[11px] tabular-nums text-faint">{r.clones || ""}</span>
      <span className={cn("size-1.5 rounded-full", r.stale ? "bg-amber" : "bg-sage")} />
    </span>
  );
}

function ServiceState() {
  const { data: on } = useService();
  if (on === undefined) return null;
  return (
    <span
      className={cn("flex items-center gap-1 text-[11px]", on ? "text-sage" : "text-faint")}
      title="Background service"
    >
      <span className={cn("size-1.5 rounded-full", on ? "bg-sage" : "border border-faint")} />
      {on ? "on" : "off"}
    </span>
  );
}

function NavItem({
  to,
  search,
  icon,
  right,
  children,
}: {
  to: "/doctor" | "/cleanup" | "/history" | "/settings";
  search?: { repo?: string };
  icon: ReactNode;
  right?: ReactNode;
  children: ReactNode;
}) {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const active = path === to;
  return (
    <Link
      to={to}
      search={search as never}
      className={cn(
        "flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] transition-colors [&_svg]:size-4",
        active
          ? "bg-foreground/[0.075] font-medium text-foreground [&_svg]:text-foreground"
          : "text-foreground/80 hover:bg-foreground/[0.045] hover:text-foreground [&_svg]:text-muted-foreground",
      )}
    >
      {icon}
      <span className="flex-1">{children}</span>
      {right}
    </Link>
  );
}

function PrefsMenu() {
  const prefs = usePrefs();
  const { data: installed } = useInstalledApps();
  const has = (id: string) => !installed || installed.includes(id as never);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] text-foreground/80 outline-none transition-colors hover:bg-foreground/[0.045] hover:text-foreground data-[state=open]:bg-foreground/[0.045] [&_svg]:size-4 [&_svg]:text-muted-foreground"
        >
          <Settings2 />
          <span className="flex-1 text-left">Open with</span>
          <span className="text-[11.5px] text-muted-foreground">{APP_LABELS[prefs.editor]}</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-52">
        <DropdownMenuLabel className="text-[11.5px] font-medium text-muted-foreground">Editor</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={prefs.editor} onValueChange={(v) => setPrefs({ editor: v as never })}>
          {EDITORS.filter(has).map((a) => (
            <DropdownMenuRadioItem key={a} value={a}>
              {APP_LABELS[a]}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-[11.5px] font-medium text-muted-foreground">Terminal</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={prefs.terminal} onValueChange={(v) => setPrefs({ terminal: v as never })}>
          {TERMINALS.filter(has).map((a) => (
            <DropdownMenuRadioItem key={a} value={a}>
              {APP_LABELS[a]}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** A small monogram tile, tinted from the name so repos are easy to tell apart. */
export function RepoMark({ name, active, className }: { name: string; active?: boolean; className?: string }) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
  const hues = [28, 128, 200, 260, 330, 60];
  const hue = hues[h % hues.length];
  return (
    <span
      className={cn(
        "flex size-[18px] shrink-0 items-center justify-center rounded-[5px] text-[10.5px] font-semibold uppercase leading-none",
        className,
      )}
      style={{
        background: `oklch(${active ? 0.62 : 0.66} 0.07 ${hue} / ${active ? 0.95 : 0.85})`,
        color: "oklch(0.99 0 0)",
      }}
    >
      {name.slice(0, 1)}
    </span>
  );
}
