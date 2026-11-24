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
import { bytes } from "@/lib/format";
import { startOp } from "@/lib/ops";
import { setUi, useUi } from "@/lib/ui";

export function RemoveCloneDialog() {
  const { remove } = useUi();
  const [deleteBranch, setDeleteBranch] = useState(false);
  const [force, setForce] = useState(false);
  const c = remove?.clone;

  useEffect(() => {
    if (remove) {
      setDeleteBranch(false);
      setForce(false);
    }
  }, [remove]);

  const dirty = !!c?.dirty;
  return (
    <AlertDialog open={!!remove} onOpenChange={(o) => !o && setUi({ remove: null })}>
      <AlertDialogContent className="max-w-[440px] gap-5 rounded-xl sm:max-w-[440px]">
        <AlertDialogHeader>
          <AlertDialogTitle className="text-[15px]">
            Remove <span className="font-mono">{c?.name}</span>?
          </AlertDialogTitle>
          <AlertDialogDescription className="text-[13px] leading-relaxed">
            Unmounts the volume, deletes the image (frees {c ? bytes(c.own_bytes ?? c.image_bytes) : ""}) and the git worktree registration.
            Commits on the branch stay in the repository.
          </AlertDialogDescription>
        </AlertDialogHeader>

        {dirty && (
          <div className="flex gap-2.5 rounded-lg border border-amber/30 bg-amber/[0.07] px-3 py-2.5 text-[12.5px] leading-relaxed">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-amber" />
            <div>
              <div className="font-medium">This clone has uncommitted changes.</div>
              <label className="mt-2 flex items-center gap-2 text-muted-foreground">
                <Switch checked={force} onCheckedChange={setForce} className="scale-[0.8]" />
                Discard them and remove anyway
              </label>
            </div>
          </div>
        )}

        {c?.branch && (
          <label className="flex items-center justify-between gap-4 rounded-lg border px-3 py-2.5">
            <span className="text-[13px]">
              Also delete branch <span className="font-mono text-[12.5px]">{c.branch}</span>
              <span className="block text-[12px] text-muted-foreground">
                {force ? "Force-deletes it, even if unmerged." : "Only if it is merged; otherwise clove keeps it."}
              </span>
            </span>
            <Switch checked={deleteBranch} onCheckedChange={setDeleteBranch} />
          </label>
        )}

        <AlertDialogFooter>
          <AlertDialogCancel className="h-8 text-[13px]">Cancel</AlertDialogCancel>
          <AlertDialogAction
            disabled={dirty && !force}
            className="h-8 bg-destructive text-[13px] text-white hover:bg-destructive/90"
            onClick={() => {
              if (!remove) return;
              void startOp({
                kind: "remove",
                root: remove.root,
                name: remove.clone.name,
                force,
                delete_branch: deleteBranch,
              });
            }}
          >
            Remove clone
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
