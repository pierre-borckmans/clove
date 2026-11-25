import { TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Switch } from "@/components/ui/switch";
import { tildify } from "@/lib/format";
import { startOp } from "@/lib/ops";
import { setUi, useUi } from "@/lib/ui";

function Blocker({ text }: { text: string }) {
  return (
    <div className="flex gap-2.5 rounded-lg border border-amber/30 bg-amber/[0.07] px-3 py-2.5 text-[12.5px] leading-relaxed">
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-amber" />
      <div className="selectable">{text}</div>
    </div>
  );
}

/** What removing would discard: the worktree's `git status --short` lines. */
function Changes({ lines, total }: { lines: string[]; total: number }) {
  const shown = lines.slice(0, 8);
  return (
    <div className="rounded-lg border px-3 py-2">
      <div className="mb-1 text-[12px] text-muted-foreground">
        {total} uncommitted change{total === 1 ? "" : "s"}
      </div>
      <pre className="selectable max-h-40 overflow-auto font-mono text-[12px] leading-relaxed">
        {shown.join("\n")}
        {total > shown.length ? `\nand ${total - shown.length} more` : ""}
      </pre>
    </div>
  );
}

/**
 * Removes a plain git worktree with `git worktree remove`. One whose only
 * problem is uncommitted changes can go with force, after they're listed.
 */
export function RemoveWorktreeDialog() {
  const { removeWorktree } = useUi();
  const [deleteBranch, setDeleteBranch] = useState(false);
  const [force, setForce] = useState(false);
  const [unlock, setUnlock] = useState(false);
  const w = removeWorktree?.worktree;
  const needsDiscard = !!w?.can_force && w.dirty === true;
  const needsUnlock = !!w?.can_force && w.locked;
  const blocked = !!w?.blocker && !(w.can_force && (!w.locked || unlock) && (w.dirty !== true || force));

  useEffect(() => {
    if (removeWorktree) {
      setDeleteBranch(false);
      setForce(false);
      setUnlock(false);
    }
  }, [removeWorktree]);

  return (
    <AlertDialog open={!!removeWorktree} onOpenChange={(o) => !o && setUi({ removeWorktree: null })}>
      <AlertDialogContent className="max-w-[440px] gap-5 rounded-xl sm:max-w-[440px]">
        <AlertDialogHeader>
          <AlertDialogTitle className="text-[15px]">
            Remove worktree <span className="font-mono">{w?.name}</span>?
          </AlertDialogTitle>
          <AlertDialogDescription className="text-[13px] leading-relaxed">
            A plain git worktree at <span className="font-mono text-[12px]">{w ? tildify(w.path) : ""}</span>. clove
            runs <span className="font-mono text-[12px]">git worktree remove</span>. Locked worktrees and commits on no
            branch always stay; uncommitted changes go only if you choose to discard them. Commits on the branch stay in
            the repository.
          </AlertDialogDescription>
        </AlertDialogHeader>

        {w?.blocker && !w.can_force && <Blocker text={w.blocker} />}

        {needsUnlock && (
          <label className="flex items-center justify-between gap-4 rounded-lg border border-amber/30 px-3 py-2.5">
            <span className="text-[13px]">
              Unlock it
              <span className="block text-[12px] text-muted-foreground">
                Locked: {w?.lock_reason ?? "no reason given"}. Runs{" "}
                <span className="font-mono">git worktree unlock</span>; the lock goes back if removal fails.
              </span>
            </span>
            <Switch checked={unlock} onCheckedChange={setUnlock} />
          </label>
        )}

        {w?.missing && (
          <div className="text-[12.5px] text-muted-foreground">
            Its folder is gone: removing drops git's record of it.
          </div>
        )}

        {needsDiscard && (
          <>
            <Changes lines={w.changes ?? []} total={w.changes_total ?? w.changes?.length ?? 0} />
            <label className="flex items-center justify-between gap-4 rounded-lg border border-amber/30 px-3 py-2.5">
              <span className="text-[13px]">
                Discard these changes
                <span className="block text-[12px] text-muted-foreground">
                  Runs <span className="font-mono">git worktree remove --force</span>. They can't be recovered.
                </span>
              </span>
              <Switch checked={force} onCheckedChange={setForce} />
            </label>
          </>
        )}

        {w?.branch && !blocked && (
          <label className="flex items-center justify-between gap-4 rounded-lg border px-3 py-2.5">
            <span className="text-[13px]">
              Also delete branch <span className="font-mono text-[12.5px]">{w.branch}</span>
              <span className="block text-[12px] text-muted-foreground">
                Only if it is merged; otherwise clove keeps it.
              </span>
            </span>
            <Switch checked={deleteBranch} onCheckedChange={setDeleteBranch} />
          </label>
        )}

        <AlertDialogFooter>
          <AlertDialogCancel className="h-8 text-[13px]">Cancel</AlertDialogCancel>
          <AlertDialogAction
            disabled={blocked}
            className="h-8 bg-destructive text-[13px] text-white hover:bg-destructive/90"
            onClick={() => {
              if (!removeWorktree) return;
              void startOp({
                kind: "remove_worktree",
                root: removeWorktree.root,
                path: removeWorktree.worktree.path,
                delete_branch: deleteBranch,
                force: force && needsDiscard,
                unlock: unlock && needsUnlock,
              });
            }}
          >
            {force && needsDiscard ? "Discard & remove" : "Remove worktree"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** Turns a plain git worktree into a clone on the same branch. */
export function AdoptDialog() {
  const { adopt } = useUi();
  const w = adopt?.worktree;
  const blocker = w?.adopt_blocker ?? (w && !w.branch ? `${w.name} is detached; a clone takes a branch` : null);
  const moving = w && !blocker && w.dirty ? (w.changes_total ?? 1) : 0;
  return (
    <AlertDialog open={!!adopt} onOpenChange={(o) => !o && setUi({ adopt: null })}>
      <AlertDialogContent className="max-w-[460px] gap-5 rounded-xl sm:max-w-[460px]">
        <AlertDialogHeader>
          <AlertDialogTitle className="text-[15px]">
            Make <span className="font-mono">{w?.name}</span> a clone?
          </AlertDialogTitle>
          <AlertDialogDescription className="text-[13px] leading-relaxed">
            clove makes a clone on <span className="font-mono text-[12px]">{w?.branch ?? "its branch"}</span>, with the
            golden image's warm dependencies, then removes this worktree with{" "}
            <span className="font-mono text-[12px]">git worktree remove</span>. Ignored files in it (build output, .env)
            go with it. If the clone fails, the worktree stays as it was.
          </AlertDialogDescription>
        </AlertDialogHeader>

        {blocker && <Blocker text={blocker} />}

        {moving > 0 && (
          <div className="rounded-lg border px-3 py-2.5 text-[12.5px] leading-relaxed">
            Its {moving} uncommitted change{moving === 1 ? "" : "s"} move{moving === 1 ? "s" : ""} to the clone, through
            a <span className="font-mono">git stash</span> (untracked files too).
            {w?.changes && w.changes.length > 0 && (
              <pre className="selectable mt-1.5 max-h-32 overflow-auto font-mono text-[12px] text-muted-foreground">
                {w.changes.slice(0, 8).join("\n")}
              </pre>
            )}
          </div>
        )}

        <AlertDialogFooter>
          <AlertDialogCancel className="h-8 text-[13px]">Cancel</AlertDialogCancel>
          <AlertDialogAction
            disabled={!!blocker}
            className="h-8 text-[13px]"
            onClick={() => {
              if (!adopt) return;
              void startOp({ kind: "adopt", root: adopt.root, path: adopt.worktree.path });
            }}
          >
            Make clone
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
