import { useQueryClient } from "@tanstack/react-query";
import { History, Trash2, Undo2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { api } from "@/lib/api";
import { bytes, shortSha, since } from "@/lib/format";
import { startOp } from "@/lib/ops";
import { checkpointNow, keys, useCheckpoints } from "@/lib/queries";
import type { Checkpoint } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";
import { cn } from "@/lib/utils";

/**
 * Picks a checkpoint to rewind a clone to, newest first. `before-rewind`
 * (what the last rewind replaced) is listed too, so a rewind can be undone.
 */
export function RewindDialog() {
  const { rewind } = useUi();
  const qc = useQueryClient();
  const root = rewind?.root;
  const clone = rewind?.clone;
  const { data, isLoading } = useCheckpoints(root, clone?.name);
  const list = [...(data ?? [])].reverse();
  const close = () => setUi({ rewind: null });

  const go = (cp: Checkpoint) => {
    if (!root || !clone) return;
    startOp({ kind: "rewind", root, name: clone.name, id: cp.id }).catch((e) =>
      toast.error(`Could not rewind ${clone.name}`, { description: String(e) }),
    );
    close();
  };
  const drop = async (cp: Checkpoint) => {
    if (!root || !clone) return;
    try {
      await api.dropCheckpoint(root, clone.name, cp.id);
    } catch (e) {
      toast.error(`Could not drop ${cp.id}`, { description: String(e) });
    }
    void qc.invalidateQueries({ queryKey: keys.checkpoints(root, clone.name) });
    void qc.invalidateQueries({ queryKey: keys.clones(root) });
  };

  return (
    <Dialog open={!!rewind} onOpenChange={(o) => !o && close()}>
      <DialogContent className="max-w-[460px] gap-4 rounded-xl sm:max-w-[460px]">
        <DialogHeader>
          <DialogTitle className="text-[15px]">
            Rewind <span className="font-mono">{clone?.name}</span>
          </DialogTitle>
          <DialogDescription className="text-[13px] leading-relaxed">
            Files, index and branch go back to the checkpoint. What it replaces is kept as{" "}
            <span className="font-mono">before-rewind</span>, so you can undo. The clone is unmounted for about a
            second.
          </DialogDescription>
        </DialogHeader>
        <ul className="max-h-72 overflow-y-auto rounded-lg border" aria-label="Checkpoints">
          {isLoading &&
            [0, 1].map((i) => (
              <li key={i} className="flex h-12 items-center gap-3 border-b px-3 last:border-b-0">
                <Skeleton className="h-3 w-24" />
                <Skeleton className="ml-auto h-3 w-14" />
              </li>
            ))}
          {!isLoading && list.length === 0 && (
            <li className="px-3 py-5 text-center text-[12.5px] text-muted-foreground">
              No checkpoints yet. Checkpoint saves this clone as it is now.
            </li>
          )}
          {list.map((cp) => (
            <li
              key={cp.id}
              className="group flex h-12 items-center gap-3 border-b px-3 last:border-b-0 hover:bg-foreground/[0.03]"
            >
              <History className={cn("size-3.5 shrink-0", cp.auto ? "text-faint" : "text-bottle")} />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                  <span className="truncate font-mono text-[12.5px] font-medium">{cp.id}</span>
                  {cp.auto && <span className="text-[11px] text-muted-foreground">undoes the last rewind</span>}
                </div>
                <div className="truncate font-mono text-[11px] text-muted-foreground">
                  {since(cp.created_at)} · {bytes(cp.own_bytes)} · {cp.branch ?? "detached"} @ {shortSha(cp.head)}
                </div>
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                className="size-7 text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-destructive"
                aria-label={`Drop checkpoint ${cp.id}`}
                onClick={() => void drop(cp)}
              >
                <Trash2 className="size-3.5" />
              </Button>
              <Button size="sm" variant="outline" className="h-7 gap-1.5 text-[12px]" onClick={() => go(cp)}>
                <Undo2 className="size-3.5" />
                Rewind
              </Button>
            </li>
          ))}
        </ul>
        <div className="flex items-center justify-between text-[11.5px] text-faint">
          <span>Size is what dropping it frees.</span>
          {root && clone && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-[12px]"
              onClick={() => void checkpointNow(qc, root, clone.name)}
            >
              Checkpoint now
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
