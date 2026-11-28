import { Code, CopyPlus, Ellipsis, FileDiff, GitBranch, RefreshCw, SquareTerminal } from "lucide-react";
import { CloneMenu, useOpenDiff } from "@/components/clone-list";
import { CloveGlyph, Glyph } from "@/components/glyph";
import { Button } from "@/components/ui/button";
import { WorktreeMenu } from "@/components/worktree-list";
import { ago, bytes, shortSha, since } from "@/lib/format";
import { startOp } from "@/lib/ops";
import { APP_LABELS, usePrefs } from "@/lib/prefs";
import { openIn, useMountToggle, usePrs } from "@/lib/queries";
import type { CloneInfo, OtherWorktree, RepoStatus } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** The bar under the map: what is picked, and what can be done with it. */
export function MapInspector({
  status,
  clones,
  picked,
  pickedWt,
  generation,
  refreshing,
}: {
  status: RepoStatus;
  clones: CloneInfo[];
  picked: CloneInfo | undefined;
  pickedWt: OtherWorktree | undefined;
  generation: number | undefined;
  refreshing: boolean;
}) {
  const prefs = usePrefs();
  const openDiff = useOpenDiff(status.root);
  const { data: prs } = usePrs(status.root);
  const mount = useMountToggle(status.root);
  const current = status.golden?.generation;
  const currentImage = current !== undefined && generation === current;
  const selectedPr = picked && prs?.get(picked.name)?.pr;
  return (
    <div className="map-inspector flex shrink-0 flex-wrap items-center gap-x-5 gap-y-2" aria-live="polite">
      {pickedWt ? (
        <>
          <div className="min-w-0 flex-1 basis-[230px]">
            <div className="flex min-w-0 items-center gap-2 text-[13px] font-medium">
              <GitBranch className="size-4 shrink-0 text-muted-foreground" />
              <span className="truncate">{pickedWt.name}</span>
              <span className="shrink-0 text-[11px] font-normal text-muted-foreground">plain git worktree</span>
            </div>
            <div className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground">
              {pickedWt.branch ?? `detached at ${shortSha(pickedWt.head)}`} · {pickedWt.path}
            </div>
            <div className="mt-1 text-[11px] text-faint">
              {pickedWt.adopt_blocker ??
                "Not from the golden image: making it a clone moves it onto copy-on-write storage, changes included."}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <Button
              size="sm"
              className="h-7 text-[12px]"
              disabled={!!pickedWt.adopt_blocker}
              onClick={() => setUi({ adopt: { root: status.root, worktree: pickedWt } })}
            >
              <CopyPlus className="size-3.5" />
              Make it a clone…
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-7 text-[12px]"
              disabled={pickedWt.missing}
              onClick={() => void openIn(pickedWt.path, prefs.editor)}
            >
              <Code className="size-3.5" />
              {APP_LABELS[prefs.editor]}
            </Button>
            <WorktreeMenu root={status.root} worktree={pickedWt}>
              <Button variant="ghost" size="icon-sm" className="size-7" aria-label="Worktree actions">
                <Ellipsis className="size-4" />
              </Button>
            </WorktreeMenu>
          </div>
        </>
      ) : (
        <>
          <div className="min-w-0 flex-1 basis-[230px]">
            <div className="flex min-w-0 items-center gap-2 text-[13px] font-medium">
              {picked ? <CloveGlyph className="h-4 w-3 shrink-0 text-bottle" /> : <Glyph className="size-4 shrink-0" />}
              <span className="truncate">
                {picked?.name ??
                  (currentImage
                    ? "Bulb (golden image)"
                    : generation !== undefined
                      ? `Generation ${generation}`
                      : "No golden image")}
              </span>
              {selectedPr && (
                <span className="shrink-0 text-[11px] font-normal text-muted-foreground">
                  #{selectedPr.number} · {selectedPr.state}
                </span>
              )}
            </div>
            <div className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground">
              {picked
                ? `${picked.branch ?? `detached at ${shortSha(picked.head)}`} · gen ${picked.golden_generation}`
                : currentImage && status.golden
                  ? `${status.golden.base} · ${shortSha(status.golden.commit)} · ${bytes(status.golden.image_bytes)}`
                  : plural(clones.filter((c) => c.golden_generation === generation).length, "clone") +
                    " created from this image"}
            </div>
            <div className="mt-1 text-[11px] text-faint">
              {picked
                ? `Created ${since(picked.created_at)}${picked.forked_from ? ` · forked from ${picked.forked_from}` : ""} · ${bytes(picked.own_bytes ?? picked.image_bytes)} ${picked.own_bytes === undefined ? "image size" : "of unique data"}${picked.checkpoints ? ` · ${plural(picked.checkpoints, "checkpoint")}` : ""}`
                : currentImage && status.golden
                  ? `Built ${ago(status.golden.age_secs)}${status.golden.behind ? ` · ${status.golden.behind} commits behind base` : status.golden.behind === null ? "" : " · up to date"}`
                  : generation === undefined
                    ? "Build an image once, then create ready worktrees from it."
                    : "The original image has been replaced; its clones remain independent."}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            {picked ? (
              <>
                {picked.mounted ? (
                  <>
                    <Button
                      size="sm"
                      className="h-7 text-[12px]"
                      onClick={() => void openIn(picked.path, prefs.editor)}
                    >
                      <Code className="size-3.5" />
                      {APP_LABELS[prefs.editor]}
                    </Button>
                    <Button
                      variant="outline"
                      size="icon-sm"
                      className="size-7"
                      aria-label="Open terminal"
                      onClick={() => void openIn(picked.path, prefs.terminal)}
                    >
                      <SquareTerminal className="size-3.5" />
                    </Button>
                    <Button variant="outline" size="sm" className="h-7 text-[12px]" onClick={() => openDiff(picked)}>
                      <FileDiff className="size-3.5" />
                      Changes
                    </Button>
                  </>
                ) : (
                  <Button
                    size="sm"
                    disabled={mount.isPending}
                    onClick={() => mount.mutate({ name: picked.name, mounted: false })}
                  >
                    Mount clone
                  </Button>
                )}
                <CloneMenu root={status.root} clone={picked}>
                  <Button variant="ghost" size="icon-sm" className="size-7" aria-label="Clone actions">
                    <Ellipsis className="size-4" />
                  </Button>
                </CloneMenu>
              </>
            ) : currentImage ? (
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-[12px]"
                disabled={refreshing}
                onClick={() => void startOp({ kind: "refresh", root: status.root })}
              >
                <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
                {refreshing ? "Refreshing…" : "Refresh"}
              </Button>
            ) : !status.golden && generation === undefined ? (
              <Button size="sm" disabled={refreshing} onClick={() => void startOp({ kind: "init", root: status.root })}>
                Build golden image
              </Button>
            ) : null}
          </div>
        </>
      )}
    </div>
  );
}
