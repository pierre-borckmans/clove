import { Check, ChevronsUpDown, Cloud, GitBranch, GitCommitHorizontal, Plus, Shuffle, Tag } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Switch } from "@/components/ui/switch";
import { since } from "@/lib/format";
import { startOp } from "@/lib/ops";
import { useClones, useRefs, useRepos } from "@/lib/queries";
import type { GitRef } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";
import { cn } from "@/lib/utils";
import { Shortcut } from "./shortcut";
import { RepoMark } from "./sidebar";

export function sanitize(branch: string) {
  return branch.replace(/[^A-Za-z0-9._-]/g, "-").replace(/^[-.]+|[-.]+$/g, "");
}

export function NewCloneDialog() {
  const { newClone } = useUi();
  const open = !!newClone;
  return (
    <Dialog open={open} onOpenChange={(o) => !o && setUi({ newClone: null })}>
      <DialogContent
        showCloseButton={false}
        className="top-[16%] max-w-[620px] translate-y-0 gap-0 overflow-hidden rounded-xl p-0 shadow-[0_24px_60px_-12px_rgb(0_0_0/0.35)] sm:max-w-[620px]"
      >
        <DialogTitle className="sr-only">New clone</DialogTitle>
        <DialogDescription className="sr-only">
          Pick a branch or type a new one. The clone is copied from the golden image.
        </DialogDescription>
        {newClone && <NewCloneForm initialRoot={newClone.root} initialBranch={newClone.branch} />}
      </DialogContent>
    </Dialog>
  );
}

function NewCloneForm({ initialRoot, initialBranch }: { initialRoot: string | null; initialBranch?: string }) {
  const { data: repos } = useRepos();
  const [root, setRoot] = useState(initialRoot ?? repos?.[0]?.root ?? "");
  const repo = repos?.find((r) => r.root === root);
  const { data: refs } = useRefs(root);
  const { data: clones } = useClones(root);
  const [query, setQuery] = useState(initialBranch ?? "");
  const [base, setBase] = useState<string | null>(null);
  const [setup, setSetup] = useState(true);

  useEffect(() => {
    if (!root && repos?.[0]) setRoot(repos[0].root);
  }, [repos, root]);

  const effectiveBase = base ?? refs?.base ?? "origin/main";
  const q = query.trim();
  const taken = new Map((clones ?? []).filter((c) => c.branch).map((c) => [c.branch as string, c.name]));

  const { local, remote } = useMemo(() => {
    const all = refs?.refs ?? [];
    const localNames = new Set(all.filter((r) => r.kind === "local").map((r) => r.name));
    const match = (n: string) => !q || n.toLowerCase().includes(q.toLowerCase());
    return {
      local: all.filter((r) => r.kind === "local" && match(r.name)).slice(0, 30),
      remote: all
        .filter((r) => r.kind === "remote" && r.name.includes("/"))
        .map((r) => ({ ...r, short: r.name.slice(r.name.indexOf("/") + 1) }))
        .filter((r) => r.short !== "HEAD" && !localNames.has(r.short) && match(r.short))
        .slice(0, 30),
    };
  }, [refs, q]);

  const exists = refs?.refs.some(
    (r) => (r.kind === "local" && r.name === q) || (r.kind === "remote" && r.name.endsWith(`/${q}`)),
  );

  const create = (opts: { branch?: string; detach?: boolean; fresh?: boolean }) => {
    if (!root) return;
    void startOp({
      kind: "new",
      root,
      branch: opts.branch ?? null,
      base: opts.fresh || opts.detach ? effectiveBase : null,
      detach: opts.detach ?? false,
      no_setup: !setup,
    });
    setUi({ newClone: null });
  };

  return (
    <Command shouldFilter={false} loop className="rounded-none bg-popover">
      <div className="flex items-center gap-2 border-b px-4 pt-3 pb-2.5 text-[12px] text-muted-foreground">
        <span>New clone in</span>
        {repos && repos.length > 1 ? (
          <RepoPicker value={root} onChange={setRoot} />
        ) : (
          <span className="font-medium text-foreground">{repo?.name}</span>
        )}
        <span className="text-faint">·</span>
        <span>copied from gen {repo?.generation ?? "?"}</span>
      </div>
      <CommandInput
        autoFocus
        value={query}
        onValueChange={setQuery}
        placeholder="Branch name, new or existing"
        className="h-12 text-[15px]"
      />
      <CommandList className="max-h-[min(360px,50vh)] [&_[cmdk-group-heading]]:pt-2.5">
        <CommandEmpty className="py-8 text-center text-[13px] text-muted-foreground">No refs match.</CommandEmpty>
        {q && !exists && (
          <CommandGroup heading="Create">
            <CommandItem value={`create:${q}`} onSelect={() => create({ branch: q, fresh: true })} className="h-10">
              <span className="flex size-6 items-center justify-center rounded-md bg-bottle-soft text-bottle">
                <Plus className="!size-3.5 !text-bottle" />
              </span>
              <span className="min-w-0 truncate">
                New branch <span className="font-mono font-medium">{q}</span>
                <span className="text-muted-foreground"> from </span>
                <span className="font-mono text-muted-foreground">{effectiveBase}</span>
              </span>
              <span className="ml-auto shrink-0 font-mono text-[11.5px] text-faint">→ {sanitize(q)}</span>
            </CommandItem>
          </CommandGroup>
        )}
        {local.length > 0 && (
          <CommandGroup heading="Branches">
            {local.map((r) => (
              <RefItem
                key={r.name}
                r={r}
                label={r.name}
                taken={taken.get(r.name)}
                onSelect={() => create({ branch: r.name })}
              />
            ))}
          </CommandGroup>
        )}
        {remote.length > 0 && (
          <CommandGroup heading="Remote branches">
            {remote.map((r) => (
              <RefItem
                key={r.name}
                r={r}
                label={r.short}
                remote
                taken={taken.get(r.short)}
                onSelect={() => create({ branch: r.short })}
              />
            ))}
          </CommandGroup>
        )}
        {!q && (
          <CommandGroup heading="Other">
            <CommandItem value="detach" onSelect={() => create({ detach: true })}>
              <GitCommitHorizontal />
              <span>
                Detached HEAD at <span className="font-mono">{effectiveBase}</span>
              </span>
              <span className="ml-auto text-[11.5px] text-faint">for reviews and experiments</span>
            </CommandItem>
            <CommandItem value="generated" onSelect={() => create({ fresh: true })}>
              <Shuffle />
              <span>New branch with a generated name</span>
            </CommandItem>
          </CommandGroup>
        )}
      </CommandList>
      <div className="flex items-center gap-4 border-t bg-muted/40 px-3 py-2">
        <BasePicker refs={refs?.refs ?? []} value={effectiveBase} onChange={setBase} />
        <label className="flex items-center gap-2 text-[12px] text-muted-foreground">
          <Switch checked={setup} onCheckedChange={setSetup} className="scale-[0.8]" />
          Run setup steps
        </label>
        <div className="flex-1" />
        <span className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
          <Shortcut keys="enter" /> create
        </span>
        <span className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
          <Shortcut keys="esc" /> close
        </span>
      </div>
    </Command>
  );
}

function RefItem({
  r,
  label,
  remote,
  taken,
  onSelect,
}: {
  r: GitRef;
  label: string;
  remote?: boolean;
  taken?: string;
  onSelect: () => void;
}) {
  return (
    <CommandItem value={`${r.kind}:${r.name}`} onSelect={onSelect} disabled={!!taken}>
      {remote ? <Cloud /> : <GitBranch />}
      <span className="shrink-0 font-mono text-[12.5px]">{label}</span>
      <span className="min-w-0 truncate text-[12px] text-faint">{r.subject}</span>
      <span className="ml-auto shrink-0 pl-3 text-[11.5px] text-faint tabular-nums">
        {taken ? `cloned as ${taken}` : since(r.date)}
      </span>
    </CommandItem>
  );
}

function BasePicker({ refs, value, onChange }: { refs: GitRef[]; value: string; onChange: (v: string) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-[12px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/30"
        >
          from <span className="font-mono text-foreground">{value}</span>
          <ChevronsUpDown className="size-3 opacity-60" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72 p-0">
        <Command>
          <CommandInput placeholder="Base ref" className="h-9 text-[13px]" />
          <CommandList className="max-h-64">
            <CommandEmpty className="py-4 text-center text-[12px]">No match.</CommandEmpty>
            <CommandGroup>
              {refs.map((r) => (
                <CommandItem
                  key={`${r.kind}:${r.name}`}
                  value={r.name}
                  onSelect={() => {
                    onChange(r.name);
                    setOpen(false);
                  }}
                  className="text-[12.5px]"
                >
                  {r.kind === "tag" ? <Tag /> : r.kind === "remote" ? <Cloud /> : <GitBranch />}
                  <span className="font-mono">{r.name}</span>
                  {r.name === value && <Check className="ml-auto" />}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

function RepoPicker({ value, onChange }: { value: string; onChange: (root: string) => void }) {
  const { data: repos } = useRepos();
  const [open, setOpen] = useState(false);
  const current = repos?.find((r) => r.root === value);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            "-mx-1 flex items-center gap-1.5 rounded px-1 py-0.5 font-medium text-foreground outline-none hover:bg-accent",
          )}
        >
          {current && <RepoMark name={current.name} className="size-4 text-[9px]" />}
          {current?.name ?? "Choose"}
          <ChevronsUpDown className="size-3 opacity-50" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-56 p-1">
        {repos?.map((r) => (
          <button
            type="button"
            key={r.id}
            onClick={() => {
              onChange(r.root);
              setOpen(false);
            }}
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-[13px] hover:bg-accent"
          >
            <RepoMark name={r.name} />
            {r.name}
            {r.root === value && <Check className="ml-auto size-3.5" />}
          </button>
        ))}
      </PopoverContent>
    </Popover>
  );
}
