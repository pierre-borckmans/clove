import { useNavigate } from "@tanstack/react-router";
import { AlertTriangle, ArrowLeft, FolderGit2, FolderOpen, Loader } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { pickFolder } from "@/lib/api";
import { ms, since, tildify } from "@/lib/format";
import { startOp, useOps } from "@/lib/ops";
import { useInitPreview, useRepoCandidates, useRepos } from "@/lib/queries";
import type { InitPreview } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";
import { OpCard, StatusIcon } from "./ops-panel";
import { Shortcut } from "./shortcut";

type Repo = { root: string; name: string };

type Stage = { kind: "choose" } | { kind: "preview"; dir: string } | { kind: "building"; repo: Repo; opId: string };

export function AddRepoDialog() {
  const { addRepo } = useUi();
  const [stage, setStage] = useState<Stage>({ kind: "choose" });

  useEffect(() => {
    if (addRepo) setStage({ kind: "choose" });
  }, [addRepo]);

  return (
    <Dialog open={addRepo} onOpenChange={(o) => !o && setUi({ addRepo: false })}>
      <DialogContent
        showCloseButton={false}
        className="top-[14%] max-w-[560px] translate-y-0 gap-0 overflow-hidden rounded-xl p-0 sm:max-w-[560px]"
      >
        <DialogTitle className="sr-only">Add repository</DialogTitle>
        <DialogDescription className="sr-only">
          Pick a git repository, see what clove will set up, then build its golden image.
        </DialogDescription>
        {stage.kind === "choose" && <ChooseStage choose={(dir) => setStage({ kind: "preview", dir })} />}
        {stage.kind === "preview" && (
          <PreviewStage
            dir={stage.dir}
            back={() => setStage({ kind: "choose" })}
            build={async (repo) => {
              const opId = await startOp({ kind: "init", root: repo.root });
              setStage({ kind: "building", repo, opId });
            }}
          />
        )}
        {stage.kind === "building" && <BuildingStage repo={stage.repo} opId={stage.opId} />}
      </DialogContent>
    </Dialog>
  );
}

const looksLikePath = (s: string) => /^[/~]/.test(s);

/** Repos found on disk, filtered as you type; a typed path or the folder picker work too. */
function ChooseStage({ choose }: { choose: (dir: string) => void }) {
  const { data: found, isLoading } = useRepoCandidates(true);
  const [query, setQuery] = useState("");
  const q = query.trim();
  const matches = useMemo(() => {
    const all = found ?? [];
    if (!q) return all;
    if (looksLikePath(q)) return all.filter((c) => tildify(c.root).startsWith(q) || c.root.startsWith(q));
    const l = q.toLowerCase();
    const first = all.filter((c) => c.name.toLowerCase().startsWith(l));
    const rest = all.filter(
      (c) => !first.includes(c) && (c.name.toLowerCase().includes(l) || tildify(c.root).toLowerCase().includes(l)),
    );
    return [...first, ...rest];
  }, [found, q]);

  const browse = async () => {
    const dir = await pickFolder();
    if (dir) choose(dir);
  };

  return (
    <Command shouldFilter={false} loop className="rounded-none bg-popover">
      <div className="border-b px-4 pt-3 pb-2.5 text-[12px] text-muted-foreground">
        Add a repository: clove builds its golden image once, then every clone takes about a second.
      </div>
      <CommandInput
        autoFocus
        value={query}
        onValueChange={setQuery}
        placeholder="Filter, or type a path like ~/code/app"
        className="h-12 text-[15px]"
      />
      <CommandList className="max-h-[min(360px,50vh)] [&_[cmdk-group-heading]]:pt-2.5">
        {!isLoading && !looksLikePath(q) && (
          <CommandEmpty className="py-8 text-center text-[13px] text-muted-foreground">
            {found?.length ? "No repo matches." : "No unmanaged git repos found in your code folders."}
          </CommandEmpty>
        )}
        {looksLikePath(q) && (
          <CommandGroup heading="Path">
            <CommandItem value={`path:${q}`} onSelect={() => choose(q)} className="h-10">
              <FolderGit2 />
              <span className="min-w-0 truncate font-mono">{q}</span>
              <span className="ml-auto shrink-0 text-[11.5px] text-faint">preview</span>
            </CommandItem>
          </CommandGroup>
        )}
        {isLoading && (
          <div className="flex items-center gap-2 px-4 py-6 text-[13px] text-muted-foreground">
            <Loader className="size-3.5 animate-spin" /> Looking for git repos in your code folders…
          </div>
        )}
        {matches.length > 0 && (
          <CommandGroup heading="Not managed yet">
            {matches.map((c) => (
              <CommandItem key={c.root} value={c.root} onSelect={() => choose(c.root)} className="h-10">
                <FolderGit2 />
                <span className="font-medium">{c.name}</span>
                {c.branch && <span className="truncate font-mono text-[11.5px] text-muted-foreground">{c.branch}</span>}
                <span className="ml-auto min-w-0 truncate font-mono text-[11.5px] text-faint">{tildify(c.root)}</span>
                <span className="w-16 shrink-0 text-right text-[11.5px] text-faint tabular-nums">
                  {since(c.active)}
                </span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}
      </CommandList>
      <div className="flex items-center gap-4 border-t bg-muted/40 px-3 py-2">
        <Button variant="ghost" size="sm" className="gap-1.5" onClick={() => void browse()}>
          <FolderOpen className="size-3.5" /> Choose folder…
        </Button>
        <div className="flex-1" />
        <span className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
          <Shortcut keys="enter" /> preview
        </span>
        <span className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
          <Shortcut keys="esc" /> close
        </span>
      </div>
    </Command>
  );
}

function Header({ repo, children }: { repo: Repo; children?: React.ReactNode }) {
  return (
    <div className="flex items-start gap-3 border-b px-5 pt-5 pb-4">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg border bg-card text-muted-foreground">
        <FolderGit2 className="size-[18px]" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[15px] font-semibold">{repo.name}</div>
        <div className="truncate font-mono text-[12px] text-muted-foreground">{tildify(repo.root)}</div>
      </div>
      {children}
    </div>
  );
}

/** What `init` will do, before it does it. */
function PreviewStage({ dir, back, build }: { dir: string; back: () => void; build: (repo: Repo) => void }) {
  const { data: p, error, isLoading } = useInitPreview(dir);
  const navigate = useNavigate();
  const { data: repos } = useRepos();
  if (isLoading)
    return (
      <div className="flex items-center gap-2 px-5 py-6 text-[13px] text-muted-foreground">
        <Loader className="size-3.5 animate-spin" /> Reading {tildify(dir)}…
      </div>
    );
  if (error || !p) return <ErrorStage message={String(error ?? "no answer")} back={back} />;
  const existing = repos?.find((r) => r.root === p.root);
  return (
    <>
      <Header repo={p} />
      <div className="max-h-[55vh] overflow-y-auto px-5 py-4">
        {p.initialized ? (
          <p className="text-[13px] leading-relaxed text-muted-foreground">clove already manages this repository.</p>
        ) : (
          <PreviewDetails p={p} />
        )}
      </div>
      <div className="flex items-center justify-end gap-2 border-t bg-muted/40 px-5 py-3">
        <Button variant="ghost" size="sm" className="mr-auto gap-1.5" onClick={back}>
          <ArrowLeft className="size-3.5" /> Back
        </Button>
        <Button variant="ghost" size="sm" onClick={() => setUi({ addRepo: false })}>
          Cancel
        </Button>
        {p.initialized ? (
          <Button
            size="sm"
            autoFocus
            onClick={() => {
              setUi({ addRepo: false });
              if (existing) void navigate({ to: "/repo/$repoId", params: { repoId: existing.id } });
            }}
          >
            Open repository
          </Button>
        ) : (
          <Button size="sm" autoFocus onClick={() => build(p)}>
            Build golden image
          </Button>
        )}
      </div>
    </>
  );
}

function PreviewDetails({ p }: { p: InitPreview }) {
  const missing = new Set(p.missing_tools);
  return (
    <>
      <p className="text-[13px] leading-relaxed text-muted-foreground">
        clove checks out <span className="font-mono text-foreground">{p.base}</span> into a new disk image and runs the
        warm steps once. This is the slow part; every clone after it takes about a second.
      </p>
      <dl className="mt-4 grid grid-cols-[88px_1fr] gap-x-2 gap-y-2 text-[12.5px]">
        <dt className="text-muted-foreground">Base</dt>
        <dd className="font-mono">
          {p.base}
          {!p.base_found && <span className="ml-2 font-sans text-amber">not found yet; init fetches it</span>}
        </dd>
        <dt className="text-muted-foreground">Warm steps</dt>
        <dd className="min-w-0">
          {p.steps.length ? (
            <ul className="space-y-1">
              {p.steps.map((s) => (
                <li key={s.name} className="flex min-w-0 items-baseline gap-2">
                  <span
                    className={`shrink-0 rounded-md border px-1.5 font-mono text-[11.5px] leading-5 ${
                      s.tool && missing.has(s.tool) ? "border-amber/60 text-amber" : ""
                    }`}
                  >
                    {s.name}
                  </span>
                  <span className="min-w-0 truncate font-mono text-[11.5px] text-muted-foreground" title={s.warm ?? ""}>
                    {s.warm ?? "in clones only"}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <span className="text-muted-foreground">none detected (checkout only)</span>
          )}
          <div className="mt-1 text-[11.5px] text-faint">
            {p.steps_detected ? "detected from the repo's files" : "from .clove.toml"}
          </div>
        </dd>
        {p.nested_skipped.length > 0 && (
          <>
            <dt className="text-muted-foreground">Not warmed</dt>
            <dd className="text-muted-foreground">
              {p.nested_skipped.length} nested projects (too many to build by default):{" "}
              <span className="font-mono text-[11.5px]">{p.nested_skipped.slice(0, 6).join(", ")}</span>
              {p.nested_skipped.length > 6 && ` and ${p.nested_skipped.length - 6} more`}
            </dd>
          </>
        )}
        {p.copy.length > 0 && (
          <>
            <dt className="text-muted-foreground">Local files</dt>
            <dd className="font-mono text-[11.5px]">{p.copy.join(", ")}</dd>
          </>
        )}
        {p.exec !== "none" && (
          <>
            <dt className="text-muted-foreground">Environment</dt>
            <dd className="font-mono">{p.exec}</dd>
          </>
        )}
        <dt className="text-muted-foreground">Clones</dt>
        <dd className="truncate font-mono text-[11.5px]">{tildify(p.clone_root)}/&lt;name&gt;</dd>
        <dt className="text-muted-foreground">Image</dt>
        <dd className="min-w-0">
          <span className="truncate font-mono text-[11.5px]">{tildify(p.state)}</span>
          <span className="ml-2 text-[11.5px] text-faint">sparse, up to {p.image_size}</span>
        </dd>
      </dl>
      {(p.missing_tools.length > 0 || p.machine.length > 0) && (
        <ul className="mt-4 space-y-1 rounded-md bg-muted px-3 py-2 text-[12px]">
          {p.missing_tools.map((t) => (
            <li key={t} className="flex items-center gap-2">
              <AlertTriangle className="size-3.5 shrink-0 text-amber" />
              <span>
                <span className="font-mono">{t}</span> isn't on PATH; its step will fail
              </span>
            </li>
          ))}
          {p.machine.map((m) => (
            <li key={m.name} className="flex items-center gap-2">
              <AlertTriangle
                className={`size-3.5 shrink-0 ${m.level === "fail" ? "text-destructive" : "text-amber"}`}
              />
              <span>
                {m.name}: {m.detail}
              </span>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function BuildingStage({ repo, opId }: { repo: Repo; opId: string }) {
  const navigate = useNavigate();
  const op = useOps().find((o) => o.id === opId);
  const { data: repos } = useRepos();
  const added = repos?.find((r) => r.root === repo.root);
  const done = op && op.status !== "running";
  return (
    <>
      <Header repo={repo}>
        {op && (
          <span className="flex items-center gap-2 pt-1 font-mono text-[12px] text-muted-foreground tabular-nums">
            <StatusIcon status={op.status} />
            {done && ms((op.finishedAt ?? 0) - op.startedAt)}
          </span>
        )}
      </Header>
      <div className="max-h-[50vh] overflow-y-auto px-5 py-4">
        {op ? (
          <OpCard op={op} embedded />
        ) : (
          <div className="flex items-center gap-2 text-[13px] text-muted-foreground">
            <Loader className="size-3.5 animate-spin" /> Starting…
          </div>
        )}
      </div>
      <div className="flex items-center justify-end gap-2 border-t bg-muted/40 px-5 py-3">
        {!done && (
          <span className="mr-auto text-[12px] text-muted-foreground">You can close this; it keeps running.</span>
        )}
        <Button variant="ghost" size="sm" onClick={() => setUi({ addRepo: false })}>
          {done ? "Close" : "Run in background"}
        </Button>
        {op?.status === "ok" && added && (
          <Button
            size="sm"
            onClick={() => {
              setUi({ addRepo: false });
              void navigate({ to: "/repo/$repoId", params: { repoId: added.id } });
            }}
          >
            Open {repo.name}
          </Button>
        )}
      </div>
    </>
  );
}

function ErrorStage({ message, back }: { message: string; back: () => void }) {
  return (
    <div className="px-5 py-5">
      <div className="text-[14px] font-semibold">Not a repository clove can use</div>
      <p className="selectable mt-2 rounded-md bg-muted px-3 py-2 font-mono text-[12px] leading-relaxed text-muted-foreground">
        {message}
      </p>
      <div className="mt-4 flex justify-end">
        <Button size="sm" variant="outline" onClick={back}>
          Back
        </Button>
      </div>
    </div>
  );
}
