import { Link, useParams, useRouterState } from "@tanstack/react-router";
import { Activity, BrushCleaning, History, Loader, Plus, Settings, Settings2, Stethoscope } from "lucide-react";
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
import { useOps } from "@/lib/ops";
import { MOD } from "@/lib/platform";
import { APP_LABELS, EDITORS, setPrefs, TERMINALS, usePrefs } from "@/lib/prefs";
import { useHistory, useInstalledApps, useRepos, useService } from "@/lib/queries";
import type { RepoSummary } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";
import { Shortcut } from "./shortcut";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

/** Widths of the sidebar, open and collapsed to a rail of icons. */
export const SIDEBAR = 236;
export const RAIL = 56;

/**
 * One layout for both widths. Every icon sits in a 40px slot centred on the
 * rail, so collapsing only narrows the sidebar over still icons: labels fade,
 * badges turn into dots on their icon, and tooltips take over the names.
 */
export function Sidebar({ compact = false }: { compact?: boolean }) {
  const { data: repos } = useRepos();
  const ops = useOps();
  const params = useParams({ strict: false }) as { repoId?: string };
  const path = useRouterState({ select: (s) => s.location.pathname });
  const running = ops.filter((o) => o.status === "running");
  const prunable = repos?.reduce((a, r) => a + r.prunable, 0) ?? 0;
  const { data: history } = useHistory();
  const autoPruned = history?.filter((e) => e.by === "auto-prune" && Date.now() / 1000 - e.at < 86_400).length ?? 0;
  const { data: serviceOn } = useService();
  const prefs = usePrefs();

  return (
    <aside className="flex h-full shrink-0 flex-col" style={{ width: SIDEBAR }}>
      <nav className="mt-2 flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto">
        <div
          className="group/section flex items-center justify-between overflow-hidden pr-4 pl-4 transition-[height] duration-200 ease-out"
          style={{ height: compact ? 8 : 24 }}
        >
          <Fade compact={compact} className="text-[11.5px] font-medium text-muted-foreground">
            Repositories
          </Fade>
          <button
            type="button"
            aria-label="Add repository"
            tabIndex={compact ? -1 : 0}
            onClick={() => setUi({ addRepo: true })}
            className={cn(
              "rounded p-0.5 text-muted-foreground opacity-0 transition-opacity hover:bg-foreground/5 hover:text-foreground",
              !compact && "group-hover/section:opacity-100",
            )}
          >
            <Plus className="size-3.5" />
          </button>
        </div>
        <div className="flex flex-col transition-[gap] duration-200 ease-out" style={{ gap: compact ? 4 : 1 }}>
          {repos?.map((r, i) => {
            const busy = running.some((o) => o.req.root === r.root) || r.refreshing;
            const active = params.repoId === r.id;
            return (
              <Item
                key={r.id}
                compact={compact}
                active={active}
                link={{ to: "/repo/$repoId", params: { repoId: r.id } }}
                icon={<RepoMark name={r.name} active={active} />}
                dot={
                  busy
                    ? null
                    : r.generation === null
                      ? undefined
                      : r.error
                        ? "bg-destructive"
                        : r.stale
                          ? "bg-amber"
                          : "bg-sage"
                }
                spinning={busy}
                big
                label={r.name}
                tip={
                  <>
                    {r.name}
                    {r.clones > 0 && <span className="ml-1.5 text-faint">{r.clones} clones</span>}
                    {i < 9 && <Shortcut keys={`mod+${i + 1}`} className="ml-1.5" />}
                  </>
                }
                right={
                  <>
                    {busy ? (
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
                  </>
                }
              />
            );
          })}
          <Item
            compact={compact}
            onClick={() => setUi({ addRepo: true })}
            muted
            big
            icon={
              <span className="flex size-[18px] items-center justify-center rounded-[5px] border border-dashed border-foreground/20">
                <Plus className="!size-3" />
              </span>
            }
            label="Add repository…"
            tip={
              <>
                Add repository… <Shortcut keys="mod+o" className="ml-1.5" />
              </>
            }
          />
        </div>
      </nav>

      <div className="flex flex-col pb-3 transition-[gap] duration-200 ease-out" style={{ gap: compact ? 4 : 1 }}>
        <Item
          compact={compact}
          active={path === "/settings"}
          link={{ to: "/settings", search: params.repoId ? { repo: params.repoId } : {} }}
          icon={<Settings />}
          label="Settings"
          dot={serviceOn ? "bg-sage" : undefined}
          tip={`Settings · background service ${serviceOn ? "on" : "off"}`}
          right={<ServiceState />}
        />
        <Item
          compact={compact}
          active={path === "/doctor"}
          link={{ to: "/doctor" }}
          icon={<Stethoscope />}
          label="Doctor"
        />
        <Item
          compact={compact}
          active={path === "/cleanup"}
          link={{ to: "/cleanup" }}
          icon={<BrushCleaning />}
          label="Cleanup"
          dot={prunable > 0 ? "bg-sage" : undefined}
          tip={prunable > 0 ? `Cleanup · ${prunable} merged` : "Cleanup"}
          right={
            prunable > 0 ? (
              <span className="rounded-full bg-sage-soft px-1.5 font-mono text-[10.5px] leading-[18px] text-sage">
                {prunable} merged
              </span>
            ) : null
          }
        />
        <Item
          compact={compact}
          active={path === "/history"}
          link={{ to: "/history" }}
          icon={<History />}
          label="History"
          dot={autoPruned > 0 ? "bg-amber" : undefined}
          tip={autoPruned > 0 ? `History · ${autoPruned} pruned today` : "History"}
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
        />
        <Item
          compact={compact}
          onClick={() => setUi({ activity: true })}
          icon={<Activity />}
          label="Activity"
          dot={running.length > 0 ? "bg-bottle" : undefined}
          tip={running.length ? `Activity · ${running.length} running` : "Activity"}
          right={
            running.length > 0 ? (
              <span className="rounded-full bg-bottle-soft px-1.5 font-mono text-[10.5px] leading-[18px] text-bottle">
                {running.length}
              </span>
            ) : null
          }
        />
        <PrefsMenu compact={compact} label={`Open with ${APP_LABELS[prefs.editor]}`} />
      </div>
    </aside>
  );
}

/** Text that fades out as the sidebar collapses; its space stays so nothing shifts. */
function Fade({ compact, className, children }: { compact: boolean; className?: string; children: ReactNode }) {
  return (
    <span
      aria-hidden={compact || undefined}
      className={cn("truncate transition-opacity duration-150", compact && "opacity-0", className)}
    >
      {children}
    </span>
  );
}

/** A collapsed item: a square, as the rail always was. */
const RAIL_ITEM = 36;

const ITEM =
  "group flex shrink-0 items-center overflow-hidden rounded-md text-[13px] transition-[width,height,margin,background-color,color] duration-200 ease-out [&_svg]:size-4";

type LinkTarget =
  | { to: "/repo/$repoId"; params: { repoId: string } }
  | { to: "/doctor" | "/cleanup" | "/history" }
  | { to: "/settings"; search: { repo?: string } };

/**
 * A sidebar row: icon slot, label, and whatever sits on the right. Its width
 * shrinks to the icon slot with the sidebar; the tooltip only shows then.
 */
function Item({
  compact,
  active = false,
  muted = false,
  link,
  onClick,
  icon,
  label,
  right,
  dot,
  spinning = false,
  big = false,
  tip,
  trigger,
}: {
  compact: boolean;
  active?: boolean;
  muted?: boolean;
  link?: LinkTarget;
  onClick?: () => void;
  icon: ReactNode;
  label: string;
  right?: ReactNode;
  /** A status dot on the icon, shown only when collapsed. */
  dot?: string | null;
  spinning?: boolean;
  /** Repo tiles grow more than line icons. */
  big?: boolean;
  tip?: ReactNode;
  /** Wraps the row, e.g. as a menu trigger. */
  trigger?: (row: ReactNode) => ReactNode;
}) {
  const className = cn(
    ITEM,
    active
      ? "bg-foreground/[0.075] font-medium text-foreground [&_svg]:text-foreground"
      : cn(
          "hover:bg-foreground/[0.045] hover:text-foreground [&_svg]:text-muted-foreground",
          muted ? "text-muted-foreground" : "text-foreground/80",
        ),
  );
  // Collapsed: 36px squares centred on the rail. Open: full-width 32px rows.
  // The icon slot keeps its centre on the rail's axis in both.
  const style = compact
    ? { width: RAIL_ITEM, height: RAIL_ITEM, marginLeft: (RAIL - RAIL_ITEM) / 2 }
    : { width: SIDEBAR - 16, height: 32, marginLeft: 8 };
  const inner = (
    <>
      <span
        className="flex shrink-0 items-center justify-center transition-[width] duration-200 ease-out"
        style={{ width: compact ? RAIL_ITEM : RAIL - 16 }}
      >
        {/* Grows with the collapse; the dot and spinner ride its corner. */}
        <span
          className="relative flex transition-transform duration-200 ease-out"
          style={{ transform: compact && big ? `scale(${22 / 18})` : undefined }}
        >
          {icon}
          {spinning && (
            <Loader
              className={cn(
                "!size-2.5 absolute -right-1 -bottom-1 animate-spin rounded-full bg-sidebar !text-bottle transition-opacity duration-200 [animation-duration:1.4s]",
                compact ? "opacity-100" : "opacity-0",
              )}
            />
          )}
          {dot && (
            <span
              className={cn(
                "absolute -top-0.5 -right-0.5 size-[5px] rounded-full ring-[1.5px] ring-sidebar transition-opacity duration-200",
                dot,
                compact ? "opacity-100" : "opacity-0",
              )}
            />
          )}
        </span>
      </span>
      <Fade compact={compact} className="min-w-0 flex-1 text-left">
        {label}
      </Fade>
      <span
        className={cn(
          "flex shrink-0 items-center gap-1.5 pr-2 transition-opacity duration-150",
          compact && "pointer-events-none opacity-0",
        )}
      >
        {right}
      </span>
    </>
  );
  const row = link ? (
    <Link
      to={link.to as never}
      params={("params" in link ? link.params : undefined) as never}
      search={("search" in link ? link.search : undefined) as never}
      aria-label={label}
      className={className}
      style={style}
    >
      {inner}
    </Link>
  ) : (
    <button type="button" aria-label={label} onClick={onClick} className={className} style={style}>
      {inner}
    </button>
  );
  // The tooltip stays mounted either way, so collapsing never remounts the row.
  return (
    <Tooltip open={compact ? undefined : false}>
      <TooltipTrigger asChild>{trigger ? trigger(row) : row}</TooltipTrigger>
      <TooltipContent side="right">{tip ?? label}</TooltipContent>
    </Tooltip>
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

function PrefsMenu({ compact, label }: { compact: boolean; label: string }) {
  const prefs = usePrefs();
  const { data: installed } = useInstalledApps();
  const has = (id: string) => !installed || installed.includes(id as never);
  return (
    <DropdownMenu>
      <Item
        compact={compact}
        icon={<Settings2 />}
        label="Open with"
        tip={label}
        right={<span className="text-[11.5px] text-muted-foreground">{APP_LABELS[prefs.editor]}</span>}
        trigger={(row) => <DropdownMenuTrigger asChild>{row}</DropdownMenuTrigger>}
      />
      <DropdownMenuContent side={compact ? "right" : "top"} align={compact ? "end" : "start"} className="w-52">
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
