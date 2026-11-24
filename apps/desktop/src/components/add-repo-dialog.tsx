import { useNavigate } from "@tanstack/react-router";
import { FolderGit2, Loader } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { api, pickFolder } from "@/lib/api";
import { ms, tildify } from "@/lib/format";
import { startOp, useOps } from "@/lib/ops";
import { useRepos } from "@/lib/queries";
import type { Discovered } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";
import { OpCard, StatusIcon } from "./ops-panel";

type Stage =
  | { kind: "picking" }
  | { kind: "error"; message: string }
  | { kind: "found"; repo: Discovered }
  | { kind: "building"; repo: Discovered; opId: string };

export function AddRepoDialog() {
  const { addRepo } = useUi();
  const [stage, setStage] = useState<Stage>({ kind: "picking" });

  useEffect(() => {
    if (!addRepo) return;
    let cancelled = false;
    setStage({ kind: "picking" });
    void (async () => {
      const dir = await pickFolder();
      if (cancelled) return;
      if (!dir) return setUi({ addRepo: false });
      try {
        setStage({ kind: "found", repo: await api.discover(dir) });
      } catch (e) {
        setStage({ kind: "error", message: String(e) });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [addRepo]);

  const busy = stage.kind === "picking";
  return (
    <Dialog open={addRepo && !busy} onOpenChange={(o) => !o && setUi({ addRepo: false })}>
      <DialogContent
        showCloseButton={false}
        className="max-w-[480px] gap-0 overflow-hidden rounded-xl p-0 sm:max-w-[480px]"
      >
        <DialogTitle className="sr-only">Add repository</DialogTitle>
        <DialogDescription className="sr-only">Build a golden image for a git repository.</DialogDescription>
        {stage.kind === "error" && <ErrorStage message={stage.message} retry={() => setUi({ addRepo: false })} />}
        {stage.kind === "found" && (
          <FoundStage
            repo={stage.repo}
            build={async () => {
              const opId = await startOp({ kind: "init", root: stage.repo.root });
              setStage({ kind: "building", repo: stage.repo, opId });
            }}
          />
        )}
        {stage.kind === "building" && <BuildingStage repo={stage.repo} opId={stage.opId} />}
      </DialogContent>
    </Dialog>
  );
}

function Header({ repo, children }: { repo: Discovered; children?: React.ReactNode }) {
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

function FoundStage({ repo, build }: { repo: Discovered; build: () => void }) {
  const navigate = useNavigate();
  const { data: repos } = useRepos();
  const existing = repos?.find((r) => r.root === repo.root);
  return (
    <>
      <Header repo={repo} />
      <div className="px-5 py-4">
        {repo.initialized ? (
          <p className="text-[13px] leading-relaxed text-muted-foreground">clove already manages this repository.</p>
        ) : (
          <>
            <p className="text-[13px] leading-relaxed text-muted-foreground">
              clove checks out <span className="font-mono text-foreground">{repo.base}</span> into a new disk image and
              runs the warm steps once. This is the slow part; every clone after it takes about a second.
            </p>
            <dl className="mt-4 grid grid-cols-[88px_1fr] gap-y-2 text-[12.5px]">
              <dt className="text-muted-foreground">Base</dt>
              <dd className="font-mono">{repo.base}</dd>
              <dt className="text-muted-foreground">Warm steps</dt>
              <dd className="flex flex-wrap gap-1.5">
                {repo.steps.length ? (
                  repo.steps.map((s) => (
                    <span key={s} className="rounded-md border px-1.5 font-mono text-[11.5px] leading-5">
                      {s}
                    </span>
                  ))
                ) : (
                  <span className="text-muted-foreground">none detected (checkout only)</span>
                )}
              </dd>
            </dl>
          </>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t bg-muted/40 px-5 py-3">
        <Button variant="ghost" size="sm" onClick={() => setUi({ addRepo: false })}>
          Cancel
        </Button>
        {repo.initialized ? (
          <Button
            size="sm"
            onClick={() => {
              setUi({ addRepo: false });
              if (existing) void navigate({ to: "/repo/$repoId", params: { repoId: existing.id } });
            }}
          >
            Open repository
          </Button>
        ) : (
          <Button size="sm" onClick={build}>
            Build golden image
          </Button>
        )}
      </div>
    </>
  );
}

function BuildingStage({ repo, opId }: { repo: Discovered; opId: string }) {
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

function ErrorStage({ message, retry }: { message: string; retry: () => void }) {
  return (
    <div className="px-5 py-5">
      <div className="text-[14px] font-semibold">Not a repository clove can use</div>
      <p className="selectable mt-2 rounded-md bg-muted px-3 py-2 font-mono text-[12px] leading-relaxed text-muted-foreground">
        {message}
      </p>
      <div className="mt-4 flex justify-end">
        <Button size="sm" variant="outline" onClick={retry}>
          Close
        </Button>
      </div>
    </div>
  );
}
