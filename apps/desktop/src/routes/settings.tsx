import { useNavigate, useSearch } from "@tanstack/react-router";
import { CircleCheck, FilePen, FilePlus, Laptop, RotateCcw, TriangleAlert } from "lucide-react";
import { type ReactNode, useState } from "react";
import { PaneHeader } from "@/components/app-shell";
import { RepoMark } from "@/components/sidebar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { tildify } from "@/lib/format";
import {
  editConfig,
  useAgents,
  useAgentsToggle,
  useConfigSet,
  useRepoConfig,
  useRepos,
  useService,
  useServiceToggle,
  useShell,
  useShellInstall,
} from "@/lib/queries";
import { type FieldState, LAYERS, type Layer, type RepoConfig } from "@/lib/types";
import { cn } from "@/lib/utils";

/** The resolved config for one repo, where each layer lives, and the machine-wide pieces. */
export function SettingsPage() {
  const { repo: repoId } = useSearch({ strict: false }) as { repo?: string };
  const navigate = useNavigate();
  const { data: repos = [] } = useRepos();
  const repo = repos.find((r) => r.id === repoId) ?? repos[0];
  const cfg = useRepoConfig(repo?.root);

  return (
    <>
      <PaneHeader>
        <h1 className="text-[14px] font-semibold tracking-tight">Settings</h1>
        <span className="text-[12.5px] text-muted-foreground">Resolved config, later files win</span>
        <div data-tauri-drag-region className="h-full flex-1" />
      </PaneHeader>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[860px] flex-col gap-6 px-6 pt-5 pb-16">
          <MachineSection stateHome={cfg.data?.state_home} />

          {repos.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5">
              {repos.map((r) => (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => void navigate({ to: "/settings", search: { repo: r.id } })}
                  className={cn(
                    "flex h-7 items-center gap-2 rounded-md border px-2.5 text-[12.5px] transition-colors",
                    r.id === repo?.id
                      ? "border-bottle/40 bg-bottle-soft font-medium text-foreground"
                      : "text-muted-foreground hover:bg-foreground/[0.045] hover:text-foreground",
                  )}
                >
                  <RepoMark name={r.name} className="size-4 text-[9.5px]" />
                  {r.name}
                </button>
              ))}
            </div>
          )}

          {repo &&
            (cfg.error ? (
              <Card>
                <div className="px-4 py-3 font-mono text-[12px] text-destructive">{String(cfg.error)}</div>
              </Card>
            ) : !cfg.data ? (
              <Skeleton className="h-64 rounded-xl" />
            ) : (
              <RepoSections cfg={cfg.data} root={repo.root} />
            ))}
        </div>
      </div>
    </>
  );
}

function MachineSection({ stateHome }: { stateHome?: string }) {
  const service = useService();
  const toggle = useServiceToggle();
  const on = toggle.isPending ? toggle.variables : service.data;
  return (
    <Section icon={<Laptop className="size-4 text-muted-foreground" />} title="This Mac">
      <Row label="Background service">
        <div className="flex items-center gap-3">
          <Switch
            checked={!!on}
            disabled={service.isLoading || toggle.isPending}
            onCheckedChange={(v) => toggle.mutate(v)}
            aria-label="Background service"
          />
          <span className="text-[13px] text-muted-foreground">
            {on
              ? "Refreshes stale golden images when you fetch or pull, and every 4h. Remounts clones at login."
              : "Off. Golden images refresh only when a new clone finds them stale."}
          </span>
        </div>
      </Row>
      <AgentsRow />
      <ShellRow />
      <Row label="State folder">
        <Mono>{stateHome ? tildify(stateHome) : "…"}</Mono>
      </Row>
    </Section>
  );
}

function AgentsRow() {
  const agents = useAgents();
  const toggle = useAgentsToggle();
  // Nothing to teach when no supported agent is installed.
  if (agents.data === null) return null;
  const state = agents.data;
  const on = toggle.isPending ? toggle.variables : state === "installed";
  return (
    <Row label="Coding agents">
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-3">
          <Switch
            checked={!!on}
            disabled={agents.isLoading || toggle.isPending}
            onCheckedChange={(v) => toggle.mutate(v)}
            aria-label="Coding agents"
          />
          <span className="text-[13px] text-muted-foreground">
            {state === "installed"
              ? "Claude Code and Codex make worktrees with clove."
              : state === "outdated"
                ? "Instructions from an older clove. Switch on to update them."
                : "Off. Agents use git worktree add and don't know clove."}
          </span>
          {state === "outdated" && (
            <Button
              variant="outline"
              size="sm"
              className="h-7 text-[12.5px]"
              disabled={toggle.isPending}
              onClick={() => toggle.mutate(true)}
            >
              Update
            </Button>
          )}
        </div>
        <span className="text-[12px] text-faint">
          A skill and a short block in <Mono className="text-faint">~/.claude/CLAUDE.md</Mono> and{" "}
          <Mono className="text-faint">~/.codex/AGENTS.md</Mono>, plus Claude Code's worktree hooks. Same as{" "}
          <Mono className="text-faint">clove agents install</Mono>.
        </span>
      </div>
    </Row>
  );
}

function ShellRow() {
  const { data: sh } = useShell();
  const install = useShellInstall();
  return (
    <Row label="Shell integration">
      <div className="flex flex-col gap-1">
        {!sh ? (
          <Plain muted>…</Plain>
        ) : sh.installed ? (
          <span className="flex items-center gap-1.5 text-[13px] text-sage">
            <CircleCheck className="size-3.5" />
            Installed in <Mono className="text-sage">{tildify(sh.rc ?? "")}</Mono>
          </span>
        ) : !sh.shell ? (
          <Plain muted>Your login shell isn't zsh, bash or fish: add `clove shell-init` to its rc by hand.</Plain>
        ) : (
          <span className="flex items-center gap-3">
            <Plain muted>Off</Plain>
            <Button
              variant="outline"
              size="sm"
              className="h-7 text-[12.5px]"
              disabled={install.isPending}
              onClick={() => install.mutate()}
            >
              Install in {tildify(sh.rc ?? "")}
            </Button>
          </span>
        )}
        <span className="text-[12px] text-faint">
          Lets <Mono className="text-faint">clove new</Mono>, <Mono className="text-faint">clove cd</Mono> and the
          terminal UI change your terminal's directory.
        </span>
      </div>
    </Row>
  );
}

function RepoSections({ cfg, root }: { cfg: RepoConfig; root: string }) {
  const c = cfg.config;
  const f = cfg.freshness;
  const [target, setTarget] = useState<Layer>("you");
  const sections = [...new Set(cfg.fields.map((x) => x.section))];
  const labelOf = (l: Layer) => cfg.layers[LAYERS.indexOf(l)]?.label ?? l;
  return (
    <>
      <Section title="Config files" subtitle="later ones win">
        {cfg.layers.map((l) => (
          <div
            key={l.path}
            className="grid grid-cols-[132px_minmax(0,1fr)_auto] items-center gap-3 border-b px-4 py-2.5 last:border-b-0"
          >
            <span className="text-[13px] text-muted-foreground">{l.label}</span>
            <span className="flex min-w-0 items-center gap-2">
              <Mono className={cn("truncate", !l.exists && "text-faint")}>{tildify(l.path)}</Mono>
              {!l.exists && <span className="shrink-0 text-[11.5px] text-faint">not created</span>}
            </span>
            <Button
              variant="outline"
              size="sm"
              className="h-7 gap-1.5 text-[12.5px]"
              onClick={() => void editConfig(l.path)}
            >
              {l.exists ? <FilePen className="size-3.5" /> : <FilePlus className="size-3.5" />}
              {l.exists ? "Edit" : "Create"}
            </Button>
          </div>
        ))}
        <Row label="Save changes to">
          <div className="flex flex-wrap gap-1.5">
            {LAYERS.map((l) => (
              <button
                key={l}
                type="button"
                aria-pressed={l === target}
                onClick={() => setTarget(l)}
                className={cn(
                  "h-7 rounded-md border px-2.5 text-[12.5px] transition-colors",
                  l === target
                    ? "border-bottle/40 bg-bottle-soft font-medium text-foreground"
                    : "text-muted-foreground hover:bg-foreground/[0.045] hover:text-foreground",
                )}
              >
                {labelOf(l)}
              </button>
            ))}
          </div>
        </Row>
      </Section>

      {sections.map((sec) => (
        <Section
          key={sec}
          title={sec[0].toUpperCase() + sec.slice(1)}
          subtitle={sec === "steps" ? (c.steps_detected ? "detected from the repo" : "from config") : undefined}
        >
          {cfg.fields
            .filter((x) => x.section === sec)
            .map((field) => (
              <FieldRow key={field.key} root={root} field={field} target={target} labelOf={labelOf} />
            ))}
          {sec === "clones" && !c.copy && cfg.copy.length > 0 && (
            <Row label="Detected files">
              <div className="flex flex-col gap-0.5">
                {cfg.copy.map((p) => (
                  <Mono key={p}>{p}</Mono>
                ))}
                <span className="text-[11.5px] text-faint">env files git ignores, copied into each clone</span>
              </div>
            </Row>
          )}
          {sec === "steps" && <StepList steps={c.steps} />}
          {sec === "freshness" && f && (
            <Row label="Now">
              {f.stale ? (
                <span className="flex items-center gap-1.5 text-[13px] text-amber">
                  <TriangleAlert className="size-3.5" />
                  Stale: {f.reason}
                </span>
              ) : (
                <span className="flex items-center gap-1.5 text-[13px] text-sage">
                  <CircleCheck className="size-3.5" />
                  Fresh
                </span>
              )}
            </Row>
          )}
        </Section>
      ))}
    </>
  );
}

/** One setting: a control for its kind, where its value comes from, and reset. */
function FieldRow({
  root,
  field,
  target,
  labelOf,
}: {
  root: string;
  field: FieldState;
  target: Layer;
  labelOf: (l: Layer) => string;
}) {
  const save = useConfigSet(root);
  const mine = field.layers[LAYERS.indexOf(target)];
  const write = (value: string | null) =>
    save.mutate({ layer: target, key: field.key, value, layerLabel: labelOf(target) });
  const overridden = mine !== null && field.source !== target;
  return (
    <div className="grid grid-cols-[132px_minmax(0,1fr)_auto] items-start gap-3 border-b px-4 py-2.5 last:border-b-0">
      <span className="pt-1 text-[13px] text-muted-foreground">{field.label}</span>
      <div className="flex min-w-0 flex-col gap-1">
        <FieldControl field={field} mine={mine} disabled={save.isPending} onSave={write} />
        <span className="text-[11.5px] text-faint">{field.help}</span>
        {overridden && field.source && (
          <span className="text-[11.5px] text-amber">
            {labelOf(target)} sets {mine || "an empty value"}, but {labelOf(field.source)} wins
          </span>
        )}
      </div>
      <div className="flex items-center gap-1.5 pt-1">
        <span className="text-[11.5px] text-faint">{field.source ? labelOf(field.source) : "default"}</span>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          disabled={mine === null || save.isPending}
          title={mine === null ? `Not set in ${labelOf(target)}` : `Remove from ${labelOf(target)}`}
          aria-label={`Reset ${field.key}`}
          onClick={() => write(null)}
        >
          <RotateCcw className="size-3.5" />
        </Button>
      </div>
    </div>
  );
}

/** The widget for a setting's kind. Text-like kinds save on Enter or blur. */
function FieldControl({
  field,
  mine,
  disabled,
  onSave,
}: {
  field: FieldState;
  mine: string | null;
  disabled: boolean;
  onSave: (v: string) => void;
}) {
  const k = field.kind;
  if (k.type === "bool") {
    return (
      <Switch
        checked={field.effective === "true"}
        disabled={disabled}
        onCheckedChange={(v) => onSave(String(v))}
        aria-label={field.label}
      />
    );
  }
  if (k.type === "choice") {
    return (
      <Select value={field.effective} disabled={disabled} onValueChange={onSave}>
        <SelectTrigger size="sm" className="h-7 w-40 text-[12.5px]" aria-label={field.label}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {k.options.map((o) => (
            <SelectItem key={o} value={o} className="text-[12.5px]">
              {o}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    );
  }
  // Start from what the target file holds, else the effective value.
  const start = mine ?? (k.type === "list" && field.source === null ? "" : field.effective);
  return <TextControl key={start} start={start} field={field} disabled={disabled} onSave={onSave} />;
}

function TextControl({
  start,
  field,
  disabled,
  onSave,
}: {
  start: string;
  field: FieldState;
  disabled: boolean;
  onSave: (v: string) => void;
}) {
  const [draft, setDraft] = useState(start);
  const commit = () => {
    if (draft.trim() !== start.trim()) onSave(draft);
  };
  return (
    <Input
      value={draft}
      disabled={disabled}
      aria-label={field.label}
      placeholder={field.kind.type === "map" ? "NAME=value, …" : field.kind.type === "list" ? "a, b, …" : undefined}
      className="h-7 max-w-[420px] font-mono text-[12px]"
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(start);
      }}
    />
  );
}

function StepList({ steps }: { steps: RepoConfig["config"]["steps"] }) {
  return (
    <>
      {steps.length === 0 && (
        <div className="px-4 py-3 text-[13px] text-muted-foreground">No steps: clones get the golden files as is.</div>
      )}
      {steps.map((s) => (
        <div key={s.name} className="border-b px-4 py-3 last:border-b-0">
          <div className="mb-1.5 text-[13px] font-medium">{s.name}</div>
          <dl className="grid grid-cols-[100px_minmax(0,1fr)] gap-x-3 gap-y-1 text-[12.5px]">
            {s.warm && (
              <>
                <dt className="text-muted-foreground">warm</dt>
                <dd>
                  <Mono>{s.warm}</Mono>
                </dd>
              </>
            )}
            {s.setup && (
              <>
                <dt className="text-muted-foreground">setup</dt>
                <dd>
                  <Mono>{s.setup}</Mono>
                </dd>
              </>
            )}
            <dt className="text-muted-foreground">reruns when</dt>
            <dd className="text-muted-foreground">
              {s.watch.length ? <Mono className="text-muted-foreground">{s.watch.join(", ")}</Mono> : "always"}
            </dd>
          </dl>
        </div>
      ))}
      <div className="px-4 py-2.5 text-[11.5px] text-faint">
        Steps are edited in a config file: [[step]] replaces the detected ones, [[add_step]] adds to them.
      </div>
    </>
  );
}

function Section({
  icon,
  title,
  subtitle,
  children,
}: {
  icon?: ReactNode;
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  return (
    <section>
      <div className="mb-2 flex items-center gap-2">
        {icon}
        <h2 className="text-[13px] font-medium">{title}</h2>
        {subtitle && <span className="text-[12px] text-faint">{subtitle}</span>}
      </div>
      <Card>{children}</Card>
    </section>
  );
}

function Card({ children }: { children: ReactNode }) {
  return <div className="overflow-hidden rounded-xl border bg-card">{children}</div>;
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[132px_minmax(0,1fr)] items-start gap-3 border-b px-4 py-2.5 last:border-b-0">
      <span className="pt-px text-[13px] text-muted-foreground">{label}</span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

function Mono({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={cn("selectable font-mono text-[12px] text-foreground/85", className)}>{children}</span>;
}

function Plain({ children, muted }: { children: ReactNode; muted?: boolean }) {
  return <span className={cn("text-[13px]", muted ? "text-muted-foreground" : "text-foreground/90")}>{children}</span>;
}
