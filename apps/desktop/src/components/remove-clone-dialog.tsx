import { GitBranch, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
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
import { bytes } from "@/lib/format";
import { startOp } from "@/lib/ops";
import type { OpRequest } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";

/** Starts one op per request; a failure is reported and the rest still go. */
export async function startEach(reqs: OpRequest[], label: (r: OpRequest) => string) {
  for (const r of reqs) {
    try {
      await startOp(r);
    } catch (e) {
      toast.error(`Could not remove ${label(r)}`, { description: String(e) });
    }
  }
}

/** Says what happens to branches: deleted unless they hold work the base lacks. */
export const BRANCH_RULE = "Unless it has unpushed commits the base lacks: then clove keeps it.";

export function RemoveCloneDialog() {
  const { remove } = useUi();
  const [deleteBranch, setDeleteBranch] = useState(true);
  const [force, setForce] = useState(false);
  const clones = remove?.clones ?? [];
  const many = clones.length > 1;
  const c = clones[0];

  useEffect(() => {
    if (remove) {
      setDeleteBranch(true);
      setForce(false);
    }
  }, [remove]);

  const dirty = clones.filter((x) => x.dirty).length;
  const branches = clones.filter((x) => x.branch).length;
  const freed = clones.reduce((n, x) => n + (x.own_bytes ?? x.image_bytes), 0);
  return (
    <AlertDialog open={!!remove} onOpenChange={(o) => !o && setUi({ remove: null })}>
      <AlertDialogContent className="max-w-[440px] gap-5 rounded-xl sm:max-w-[440px]">
        <AlertDialogHeader>
          <AlertDialogTitle className="text-[15px]">
            {many ? (
              `Remove ${clones.length} clones?`
            ) : (
              <>
                Remove <span className="font-mono">{c?.name}</span>?
              </>
            )}
          </AlertDialogTitle>
          <AlertDialogDescription className="text-[13px] leading-relaxed">
            Unmounts the {many ? "volumes" : "volume"}, deletes the {many ? "images" : "image"} (frees {bytes(freed)})
            and the git worktree registration.
          </AlertDialogDescription>
        </AlertDialogHeader>

        {many && (
          <ul className="selectable max-h-44 overflow-auto rounded-lg border px-3 py-2 text-[12.5px] leading-relaxed">
            {clones.map((x) => (
              <li key={x.name} className="flex min-w-0 items-center gap-2">
                <span className="truncate font-mono">{x.name}</span>
                {x.branch && (
                  <span className="flex min-w-0 items-center gap-1 font-mono text-[11.5px] text-muted-foreground">
                    <GitBranch className="size-3 shrink-0 text-faint" />
                    <span className="truncate">{x.branch}</span>
                  </span>
                )}
                {x.dirty && <span className="shrink-0 text-[11.5px] text-amber">modified</span>}
              </li>
            ))}
          </ul>
        )}

        {dirty > 0 && (
          <div className="flex gap-2.5 rounded-lg border border-amber/30 bg-amber/[0.07] px-3 py-2.5 text-[12.5px] leading-relaxed">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-amber" />
            <div>
              <div className="font-medium">
                {many
                  ? `${dirty} of them ${dirty === 1 ? "has" : "have"} uncommitted changes; without discarding, ${dirty === 1 ? "it stays" : "they stay"}.`
                  : "This clone has uncommitted changes."}
              </div>
              <label className="mt-2 flex items-center gap-2 text-muted-foreground">
                <Switch checked={force} onCheckedChange={setForce} className="scale-[0.8]" />
                Discard them and remove anyway
              </label>
            </div>
          </div>
        )}

        {branches > 0 && (
          <label className="flex items-center justify-between gap-4 rounded-lg border px-3 py-2.5">
            <span className="text-[13px]">
              {many ? (
                "Also delete their branches"
              ) : (
                <>
                  Also delete branch <span className="font-mono text-[12.5px]">{c?.branch}</span>
                </>
              )}
              <span className="block text-[12px] text-muted-foreground">{BRANCH_RULE}</span>
            </span>
            <Switch checked={deleteBranch} onCheckedChange={setDeleteBranch} />
          </label>
        )}

        <AlertDialogFooter>
          <AlertDialogCancel className="h-8 text-[13px]">Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={!many && dirty > 0 && !force}
            className="h-8 text-[13px]"
            onClick={() => {
              if (!remove) return;
              const root = remove.root;
              // Without force, modified ones stay (the dialog said so).
              void startEach(
                clones
                  .filter((x) => force || !x.dirty)
                  .map((x) => ({
                    kind: "remove",
                    root,
                    name: x.name,
                    // Force only discards changes the user saw flagged.
                    force: force && !!x.dirty,
                    delete_branch: deleteBranch,
                  })),
                (r) => (r.kind === "remove" ? r.name : ""),
              );
            }}
          >
            {many ? `Remove ${clones.length} clones` : "Remove clone"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
