import { useNavigate, useSearch } from "@tanstack/react-router";
import { CircleCheck, FilePen, FilePlus, Laptop, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { PaneHeader } from "@/components/app-shell";
import { RepoMark } from "@/components/sidebar";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { ago, tildify } from "@/lib/format";
import {
  editConfig,
  useRepoConfig,
  useRepos,
  useService,
  useServiceToggle,
  useShell,
  useShellInstall,
} from "@/lib/queries";
import type { RepoConfig } from "@/lib/types";
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
              <RepoSections cfg={cfg.data} />
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
      <ShellRow />
      <Row label="State folder">
        <Mono>{stateHome ? tildify(stateHome) : "…"}</Mono>
      </Row>
    </Section>
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
          Lets <Mono className="text-faint">clove new</Mono>, <Mono className="text-faint">clove cd</Mono> and{" "}
          <Mono className="text-faint">clove ui</Mono> change your terminal's directory.
        </span>
      </div>
    </Row>
  );
}

function RepoSections({ cfg }: { cfg: RepoConfig }) {
  const c = cfg.config;
  const f = cfg.freshness;
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
      </Section>

      <Section title="Clones">
        <Row label="Base">
          <Mono>{c.base}</Mono>
        </Row>
        <Row label="Live in">
          <Mono>{tildify(c.clone_root)}</Mono>
        </Row>
        <Row label="Image size">
          <Plain>{c.image_size} (sparse, only used blocks cost disk)</Plain>
        </Row>
        <Row label="Fetch">
          <Plain>
            {c.fetch ? `Before each new clone, at most every ${ago(c.fetch_interval).replace(" ago", "")}` : "Off"}
          </Plain>
        </Row>
        <Row label="Environment">
          <Plain>{c.exec}</Plain>
        </Row>
        {Object.entries(c.env).map(([k, v]) => (
          <Row key={k} label="Env">
            <Mono>
              {k}={v}
            </Mono>
          </Row>
        ))}
        <Row label="Local files">
          {cfg.copy.length === 0 ? (
            <Plain muted>None ({c.copy ? "configured" : "detected"})</Plain>
          ) : (
            <div className="flex flex-col gap-0.5">
              {cfg.copy.map((p) => (
                <Mono key={p}>{p}</Mono>
              ))}
              <span className="text-[11.5px] text-faint">
                {c.copy ? "configured" : "env files git ignores, detected"}, copied into each clone
              </span>
            </div>
          )}
        </Row>
      </Section>

      <Section title="Steps" subtitle={c.steps_detected ? "detected from the repo" : "from config"}>
        {c.steps.length === 0 && (
          <div className="px-4 py-3 text-[13px] text-muted-foreground">
            No steps: clones get the golden files as is.
          </div>
        )}
        {c.steps.map((s) => (
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
      </Section>

      <Section title="Freshness">
        <Row label="Stale when">
          <Plain>
            A step's watched files change, or after {ago(c.refresh.max_age_secs).replace(" ago", "")} /{" "}
            {c.refresh.max_behind} commits
          </Plain>
        </Row>
        <Row label="Auto refresh">
          <Plain>{c.refresh.auto ? "On, when a new clone finds the image stale" : "Off"}</Plain>
        </Row>
        {f && (
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
