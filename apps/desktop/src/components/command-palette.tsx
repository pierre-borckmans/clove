import { useQueries } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import {
  Activity,
  ArrowLeft,
  BrushCleaning,
  Check,
  Code,
  Copy,
  FolderOpen,
  FolderPlus,
  GitBranch,
  HardDrive,
  History,
  Plus,
  Power,
  RefreshCw,
  Settings,
  Settings2,
  SquareTerminal,
  Stethoscope,
  Trash2,
  Unplug,
} from "lucide-react";
import { useState } from "react";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command";
import { api } from "@/lib/api";
import { startOp } from "@/lib/ops";
import { APP_LABELS, EDITORS, setPrefs, TERMINALS, usePrefs } from "@/lib/prefs";
import {
  copyPath,
  keys,
  openIn,
  useInstalledApps,
  useMountToggle,
  useRepos,
  useService,
  useServiceToggle,
} from "@/lib/queries";
import type { CloneInfo, RepoSummary } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";
import { RepoMark } from "./sidebar";

type Page = { kind: "root" } | { kind: "clone"; repo: RepoSummary; clone: CloneInfo } | { kind: "apps" };

export function CommandPalette() {
  const { palette } = useUi();
  const [page, setPage] = useState<Page>({ kind: "root" });
  const [query, setQuery] = useState("");

  const close = () => {
    setUi({ palette: false });
    setTimeout(() => {
      setPage({ kind: "root" });
      setQuery("");
    }, 150);
  };
  const go = (p: Page) => {
    setPage(p);
    setQuery("");
  };

  return (
    <CommandDialog
      open={palette}
      onOpenChange={(o) => (o ? setUi({ palette: true }) : close())}
      title="Command palette"
      description="Jump to a repository or clone, or run an action"
      showCloseButton={false}
      className="top-[16%] max-w-[620px] translate-y-0 rounded-xl shadow-[0_24px_60px_-12px_rgb(0_0_0/0.35)] sm:max-w-[620px] [&_[cmdk-item]]:py-2 [&_[cmdk-item]_svg]:!size-4"
    >
      <div
        onKeyDownCapture={(e) => {
          if (page.kind !== "root" && (e.key === "Escape" || (e.key === "Backspace" && !query))) {
            e.preventDefault();
            e.stopPropagation();
            go({ kind: "root" });
          }
        }}
      >
        {page.kind !== "root" && (
          <div className="flex items-center gap-2 px-3 pt-3 text-[12px] text-muted-foreground">
            <button
              type="button"
              onClick={() => go({ kind: "root" })}
              className="flex items-center gap-1 rounded px-1 py-0.5 hover:bg-accent hover:text-foreground"
            >
              <ArrowLeft className="size-3" />
              Back
            </button>
            <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[11.5px] text-foreground">
              {page.kind === "clone" ? `${page.repo.name} / ${page.clone.name}` : "Open with"}
            </span>
          </div>
        )}
        <CommandInput
          value={query}
          onValueChange={setQuery}
          placeholder={page.kind === "root" ? "Search repositories, clones and actions" : "Search actions"}
          className="text-[14px]"
        />
        <CommandList className="max-h-[min(420px,55vh)]">
          <CommandEmpty className="py-8 text-center text-[13px] text-muted-foreground">No results.</CommandEmpty>
          {page.kind === "root" && <RootPage go={go} close={close} />}
          {page.kind === "clone" && <ClonePage repo={page.repo} clone={page.clone} close={close} />}
          {page.kind === "apps" && <AppsPage close={close} />}
        </CommandList>
      </div>
    </CommandDialog>
  );
}

function RootPage({ go, close }: { go: (p: Page) => void; close: () => void }) {
  const navigate = useNavigate();
  const { data: repos = [] } = useRepos();
  const params = useParams({ strict: false }) as { repoId?: string };
  const current = repos.find((r) => r.id === params.repoId);
  const prefs = usePrefs();
  const { data: serviceOn } = useService();
  const service = useServiceToggle();
  const clones = useQueries({
    queries: repos.map((r) => ({
      queryKey: keys.clones(r.root),
      queryFn: () => api.clones(r.root, true),
    })),
  });

  const run = (f: () => void) => () => {
    close();
    f();
  };

  return (
    <>
      <CommandGroup heading="Actions">
        <CommandItem onSelect={run(() => setUi({ newClone: { root: current?.root ?? repos[0]?.root ?? null } }))}>
          <Plus /> New clone{current ? ` in ${current.name}` : ""}…<CommandShortcut>⌘N</CommandShortcut>
        </CommandItem>
        {current && (
          <CommandItem onSelect={run(() => void startOp({ kind: "refresh", root: current.root }))}>
            <RefreshCw /> Refresh golden image of {current.name}
          </CommandItem>
        )}
        <CommandItem onSelect={run(() => setUi({ addRepo: true }))}>
          <FolderPlus /> Add repository…<CommandShortcut>⌘O</CommandShortcut>
        </CommandItem>
        <CommandItem onSelect={run(() => void navigate({ to: "/doctor" }))}>
          <Stethoscope /> Run doctor
        </CommandItem>
        <CommandItem
          value="settings config preferences"
          onSelect={run(() => void navigate({ to: "/settings", search: current ? { repo: current.id } : {} }))}
        >
          <Settings /> Settings{current ? ` for ${current.name}` : ""}
          <CommandShortcut>⌘,</CommandShortcut>
        </CommandItem>
        <CommandItem
          value="cleanup prune merged clones leftovers gc"
          onSelect={run(() => void navigate({ to: "/cleanup" }))}
        >
          <BrushCleaning /> Clean up merged clones and leftovers
        </CommandItem>
        <CommandItem
          value="history log removed pruned auto-prune restore branch"
          onSelect={run(() => void navigate({ to: "/history" }))}
        >
          <History /> What clove removed
        </CommandItem>
        <CommandItem value="service background refresh launchd toggle" onSelect={run(() => service.mutate(!serviceOn))}>
          <Power /> {serviceOn ? "Turn off the background service" : "Turn on the background service"}
        </CommandItem>
        <CommandItem onSelect={run(() => setUi({ activity: true }))}>
          <Activity /> Show recent activity
        </CommandItem>
        <CommandItem value="open with editor terminal preferences" onSelect={() => go({ kind: "apps" })}>
          <Settings2 /> Open clones with…
          <CommandShortcut className="tracking-normal">
            {APP_LABELS[prefs.editor]}, {APP_LABELS[prefs.terminal]}
          </CommandShortcut>
        </CommandItem>
      </CommandGroup>
      <CommandGroup heading="Repositories">
        {repos.map((r, i) => (
          <CommandItem
            key={r.id}
            value={`repo ${r.name} ${r.root}`}
            onSelect={run(() => void navigate({ to: "/repo/$repoId", params: { repoId: r.id } }))}
          >
            <RepoMark name={r.name} />
            {r.name}
            <span className="text-[12px] text-faint">
              {r.clones} clone{r.clones === 1 ? "" : "s"}
            </span>
            {i < 9 && <CommandShortcut>⌘{i + 1}</CommandShortcut>}
          </CommandItem>
        ))}
      </CommandGroup>
      <CommandGroup heading="Clones">
        {repos.flatMap((r, i) =>
          (clones[i]?.data ?? []).map((c) => (
            <CommandItem
              key={`${r.id}/${c.name}`}
              value={`clone ${r.name} ${c.name} ${c.branch ?? ""}`}
              onSelect={() => go({ kind: "clone", repo: r, clone: c })}
            >
              <GitBranch />
              <span className="font-medium">{c.name}</span>
              {c.branch && c.branch !== c.name && (
                <span className="truncate font-mono text-[12px] text-muted-foreground">{c.branch}</span>
              )}
              <span className="ml-auto text-[12px] text-faint">{r.name}</span>
            </CommandItem>
          )),
        )}
      </CommandGroup>
    </>
  );
}

function ClonePage({ repo, clone: c, close }: { repo: RepoSummary; clone: CloneInfo; close: () => void }) {
  const prefs = usePrefs();
  const mount = useMountToggle(repo.root);
  const run = (f: () => void) => () => {
    close();
    f();
  };
  return (
    <CommandGroup heading={c.branch ?? "detached"}>
      <CommandItem onSelect={run(() => void openIn(c.path, prefs.editor))}>
        <Code /> Open in {APP_LABELS[prefs.editor]}
      </CommandItem>
      <CommandItem onSelect={run(() => void openIn(c.path, prefs.terminal))}>
        <SquareTerminal /> Open in {APP_LABELS[prefs.terminal]}
      </CommandItem>
      <CommandItem onSelect={run(() => void openIn(c.path, "finder"))}>
        <FolderOpen /> Reveal in Finder
      </CommandItem>
      <CommandItem onSelect={run(() => void copyPath(c.path))}>
        <Copy /> Copy path
      </CommandItem>
      <CommandItem onSelect={run(() => mount.mutate({ name: c.name, mounted: c.mounted }))}>
        {c.mounted ? <Unplug /> : <HardDrive />} {c.mounted ? "Unmount" : "Mount"}
      </CommandItem>
      <CommandItem onSelect={run(() => setUi({ remove: { root: repo.root, clone: c } }))} className="text-destructive">
        <Trash2 className="!text-destructive" /> Remove clone…
      </CommandItem>
    </CommandGroup>
  );
}

function AppsPage({ close }: { close: () => void }) {
  const prefs = usePrefs();
  const { data: installed } = useInstalledApps();
  const has = (a: string) => !installed || installed.includes(a as never);
  return (
    <>
      <CommandGroup heading="Editor">
        {EDITORS.filter(has).map((a) => (
          <CommandItem
            key={a}
            value={`editor ${a}`}
            onSelect={() => {
              setPrefs({ editor: a });
              close();
            }}
          >
            <Code /> {APP_LABELS[a]}
            {prefs.editor === a && <Check className="ml-auto" />}
          </CommandItem>
        ))}
      </CommandGroup>
      <CommandGroup heading="Terminal">
        {TERMINALS.filter(has).map((a) => (
          <CommandItem
            key={a}
            value={`terminal ${a}`}
            onSelect={() => {
              setPrefs({ terminal: a });
              close();
            }}
          >
            <SquareTerminal /> {APP_LABELS[a]}
            {prefs.terminal === a && <Check className="ml-auto" />}
          </CommandItem>
        ))}
      </CommandGroup>
    </>
  );
}
