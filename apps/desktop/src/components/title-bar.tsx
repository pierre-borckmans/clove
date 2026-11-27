import { useQuery } from "@tanstack/react-query";
import { Link, useParams, useRouterState } from "@tanstack/react-router";
import { ChevronRight, Loader, Monitor, Moon, PanelLeft, Search, Sun } from "lucide-react";
import type { ReactNode } from "react";
import { cycleAppearance } from "@/hooks/use-theme";
import { isTauri } from "@/lib/bridge";
import { useOps } from "@/lib/ops";
import { isMac } from "@/lib/platform";
import { setPrefs, usePrefs } from "@/lib/prefs";
import { useRepos, useService } from "@/lib/queries";
import { getUi, setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";
import { Glyph } from "./glyph";
import { Shortcut } from "./shortcut";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

/**
 * The macOS traffic lights sit centred 20px from the window top, x 18 to 70,
 * so the bar is 40px and everything in it shares their line.
 */
export const BAR = 40;
const LIGHTS = isMac ? 80 : 12;

const PAGES: Record<string, string> = {
  "/doctor": "Doctor",
  "/cleanup": "Cleanup",
  "/history": "History",
  "/settings": "Settings",
};

/**
 * The window's title bar, always there: where you are on the left, search in
 * the middle, what clove is doing on the right. All of it drags the window.
 */
export function TitleBar() {
  const collapsed = usePrefs().sidebarCollapsed;
  return (
    <div
      data-tauri-drag-region
      className="relative grid shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 pr-3"
      style={{ height: BAR, paddingLeft: LIGHTS }}
    >
      {/* Stand-ins for the traffic lights when the UI runs in a browser. */}
      {!isTauri && isMac && (
        <span className="pointer-events-none absolute top-1/2 left-[18px] flex -translate-y-1/2 gap-2">
          {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
            <span key={c} className="size-3 rounded-full" style={{ background: c }} />
          ))}
        </span>
      )}
      <div data-tauri-drag-region className="flex min-w-0 items-center gap-1.5">
        <BarButton
          label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          keys="mod+b"
          onClick={() => setPrefs({ sidebarCollapsed: !collapsed })}
        >
          <PanelLeft />
        </BarButton>
        <Crumbs />
      </div>
      <button
        type="button"
        onClick={() => setUi({ palette: true })}
        className="flex h-[26px] w-[clamp(220px,32vw,420px)] items-center gap-2 rounded-md border border-sidebar-line bg-background/50 px-2.5 text-[12.5px] text-muted-foreground transition-colors hover:bg-background/80 hover:text-foreground"
      >
        <Search className="size-3.5 shrink-0" />
        <span className="flex-1 truncate text-left">Search or run a command</span>
        <Shortcut keys="mod+k" className="[&_kbd]:h-4 [&_kbd]:min-w-4 [&_kbd]:text-[10px]" />
      </button>
      <div data-tauri-drag-region className="flex items-center justify-end gap-3">
        <Running />
        <Service />
        <AppearanceButton />
        <Version />
      </div>
    </div>
  );
}

/** Where you are: clove › repo › clone › page, each step a link back up. */
function Crumbs() {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const params = useParams({ strict: false }) as { repoId?: string; clone?: string };
  const repo = useRepos().data?.find((r) => r.id === params.repoId);
  const parts: ReactNode[] = [];
  if (repo) {
    parts.push(
      <Link key="r" to="/repo/$repoId" params={{ repoId: repo.id }} className={cn(CRUMB, "shrink-0")}>
        {repo.name}
      </Link>,
    );
  }
  if (params.clone) {
    parts.push(
      <span key="c" className="truncate font-mono text-[12px] text-foreground/80">
        {params.clone}
      </span>,
      <span key="p" className="shrink-0 text-foreground/80">
        Changes
      </span>,
    );
  }
  if (PAGES[path]) parts.push(<span key="p">{PAGES[path]}</span>);
  return (
    <nav aria-label="Location" className="flex min-w-0 items-center gap-1 text-[12.5px] text-muted-foreground">
      <Link to="/" className={cn(CRUMB, "flex shrink-0 items-center gap-1.5 font-medium text-foreground/90")}>
        <Glyph className="size-[15px]" />
        clove
      </Link>
      {parts.map((p, i) => (
        <span
          key={(p as { key: string }).key ?? i}
          className={cn("flex items-center gap-1", (p as { key: string }).key === "c" ? "min-w-0" : "shrink-0")}
        >
          <ChevronRight className="size-3 shrink-0 text-faint" />
          {p}
        </span>
      ))}
    </nav>
  );
}

const CRUMB = "truncate rounded px-1 py-0.5 transition-colors hover:bg-foreground/[0.06] hover:text-foreground";

/** Operations in flight; opens the activity panel. */
function Running() {
  const running = useOps().filter((o) => o.status === "running");
  if (running.length === 0) return null;
  return (
    <button
      type="button"
      onClick={() => setUi({ activity: !getUi().activity })}
      className="flex items-center gap-1.5 rounded-md px-1.5 py-0.5 text-[12px] text-bottle transition-colors hover:bg-bottle-soft"
    >
      <Loader className="size-3 animate-spin [animation-duration:1.4s]" />
      {running.length} running
    </button>
  );
}

function Service() {
  const { data: on } = useService();
  if (on === undefined) return null;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Link
          to="/settings"
          className={cn(
            "flex items-center gap-1.5 rounded-md px-1.5 py-0.5 text-[12px] transition-colors hover:bg-foreground/[0.06]",
            on ? "text-muted-foreground" : "text-amber",
          )}
        >
          <span className={cn("size-1.5 rounded-full", on ? "bg-sage" : "bg-amber")} />
          {on ? "service on" : "service off"}
        </Link>
      </TooltipTrigger>
      <TooltipContent side="bottom">
        {on
          ? "The background service keeps golden images fresh and prunes merged clones."
          : "The background service is off: golden images go stale until you refresh them."}
      </TooltipContent>
    </Tooltip>
  );
}

const APPEARANCE = {
  system: { icon: <Monitor />, label: "Appearance: system" },
  light: { icon: <Sun />, label: "Appearance: light" },
  dark: { icon: <Moon />, label: "Appearance: dark" },
} as const;

/** Cycles system, light, dark; the icon shows the current choice. */
function AppearanceButton() {
  const a = APPEARANCE[usePrefs().appearance];
  return (
    <BarButton label={a.label} keys="mod+shift+l" onClick={cycleAppearance}>
      {a.icon}
    </BarButton>
  );
}

function Version() {
  const { data } = useQuery({
    queryKey: ["app-version"],
    queryFn: async () => (isTauri ? (await import("@tauri-apps/api/app")).getVersion() : "dev"),
    staleTime: Number.POSITIVE_INFINITY,
  });
  return data ? <span className="font-mono text-[11px] text-faint">v{data}</span> : null;
}

function BarButton({
  label,
  keys,
  onClick,
  children,
}: {
  label: string;
  keys?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          onClick={onClick}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground [&_svg]:size-4"
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom">
        {label}
        {keys && <Shortcut keys={keys} className="ml-1.5" />}
      </TooltipContent>
    </Tooltip>
  );
}
