import { useQueryClient } from "@tanstack/react-query";
import { ArrowUpRight, CircleAlert, Loader, RotateCw, TriangleAlert } from "lucide-react";
import { type ReactNode, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { keys, useCheckUpgradeNow, useUpgrade } from "@/lib/queries";
import type { UpgradeState } from "@/lib/types";
import { setUi, useUi } from "@/lib/ui";
import { cn } from "@/lib/utils";

/** The CLI on PATH runs a different version than this app. */
export function skew(s: UpgradeState): string | null {
  return s.cli_version && s.cli_version !== s.current ? s.cli_version : null;
}

/**
 * New release, release notes and the way to get it: in place for Homebrew
 * installs (the app quits, `clove upgrade` replaces it and opens it again),
 * the download page otherwise.
 */
export function UpgradeDialog() {
  const { upgrade: open } = useUi();
  const { data: s } = useUpgrade();
  const qc = useQueryClient();
  const check = useCheckUpgradeNow();
  const [updating, setUpdating] = useState(false);

  const close = () => {
    setUi({ upgrade: false });
    if (s?.upgrade_error) {
      void api.clearUpgradeError().then(() => qc.invalidateQueries({ queryKey: keys.upgrade }));
    }
  };

  if (!s) return null;
  const latest = s.latest;
  const inPlace = s.app_source === "brew" && s.cli === "brew";
  const cliSkew = skew(s);

  const update = async () => {
    setUpdating(true);
    try {
      await api.startUpgrade();
    } catch (e) {
      setUpdating(false);
      toast.error("Could not start the update", { description: String(e) });
    }
  };

  let title: string;
  let lead: ReactNode;
  let action: ReactNode = null;
  if (s.restart_to) {
    title = `clove ${s.restart_to} is installed`;
    lead = `This window still runs ${s.current}. Restart to use the new version.`;
    action = (
      <Button size="sm" className="h-8 text-[13px]" onClick={() => void api.restartApp()}>
        <RotateCw /> Restart clove
      </Button>
    );
  } else if (s.available && latest) {
    title = `clove ${latest.version} is available`;
    lead = inPlace ? (
      `You have ${s.current}. clove quits, upgrades with Homebrew, and opens again.`
    ) : (
      <>
        You have {s.current}.{" "}
        {s.app_source === "brew"
          ? `The clove CLI${s.cli_path ? ` at ${s.cli_path}` : ""} isn't from Homebrew, so update by hand.`
          : "This app wasn't installed with Homebrew: download the new version."}
      </>
    );
    action = inPlace ? (
      <Button size="sm" className="h-8 text-[13px]" disabled={updating} onClick={() => void update()}>
        {updating && <Loader className="animate-spin" />}
        {updating ? "Updating…" : "Update and restart"}
      </Button>
    ) : (
      <Button size="sm" className="h-8 text-[13px]" onClick={() => void api.openRelease(latest.url)}>
        Download <ArrowUpRight />
      </Button>
    );
  } else {
    title = `clove ${s.current} is up to date`;
    lead = s.error ? `The last check failed: ${s.error}` : "There is no newer release.";
    action = (
      <Button
        size="sm"
        variant="outline"
        className="h-8 text-[13px]"
        disabled={check.isPending}
        onClick={() => check.mutate()}
      >
        {check.isPending && <Loader className="animate-spin" />}
        Check again
      </Button>
    );
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="max-w-[520px] gap-4 rounded-xl sm:max-w-[520px]">
        <div className="grid gap-1.5">
          <DialogTitle className="text-[15px]">{title}</DialogTitle>
          <DialogDescription className="text-[13px] leading-relaxed">{lead}</DialogDescription>
        </div>

        {s.upgrade_error && (
          <Notice tone="error" icon={<CircleAlert />}>
            <div className="font-medium">The last update failed; this is still {s.current}.</div>
            <pre className="mt-1 max-h-28 overflow-auto font-mono text-[11.5px] whitespace-pre-wrap text-muted-foreground">
              {s.upgrade_error}
            </pre>
            <div className="mt-1 text-muted-foreground">Full output: ~/.clove/logs/upgrade.log</div>
          </Notice>
        )}

        {cliSkew && !s.restart_to && (
          <Notice tone="warn" icon={<TriangleAlert />}>
            The <span className="font-mono">clove</span> CLI is {cliSkew} and this app is {s.current}. The background
            service and agent hooks run the CLI, so keep them on the same version.
          </Notice>
        )}

        {s.available && latest?.notes && (
          <div className="max-h-[300px] overflow-auto rounded-lg border bg-muted/30 px-3.5 py-3">
            <Notes text={latest.notes} />
          </div>
        )}

        {updating && (
          <p className="text-[12px] text-muted-foreground">
            clove closes now. If it doesn't come back, see ~/.clove/logs/upgrade.log.
          </p>
        )}

        <div className="flex items-center justify-between gap-2">
          {latest ? (
            <Button
              variant="link"
              size="sm"
              className="h-8 px-0 text-[12.5px] text-muted-foreground"
              onClick={() => void api.openRelease(latest.url)}
            >
              Release on GitHub <ArrowUpRight />
            </Button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <Button variant="outline" size="sm" className="h-8 text-[13px]" onClick={close}>
              {s.available || s.restart_to ? "Later" : "Close"}
            </Button>
            {action}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Notice({ tone, icon, children }: { tone: "warn" | "error"; icon: ReactNode; children: ReactNode }) {
  const [box, ink] =
    tone === "warn"
      ? ["border-amber/30 bg-amber/[0.07]", "text-amber"]
      : ["border-destructive/30 bg-destructive/[0.06]", "text-destructive"];
  return (
    <div className={cn("flex gap-2.5 rounded-lg border px-3 py-2.5 text-[12.5px] leading-relaxed", box)}>
      <span className={cn("mt-0.5 shrink-0 [&_svg]:size-4", ink)}>{icon}</span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

/** GitHub's generated notes: headings, bullets and bold, nothing fancier. */
function Notes({ text }: { text: string }) {
  const plain = (s: string) => s.replace(/\*\*(.+?)\*\*/g, "$1").replace(/`([^`]+)`/g, "$1");
  return (
    <div className="grid gap-1 text-[12.5px] leading-relaxed">
      {text
        .split("\n")
        .map((l) => l.trimEnd())
        .filter((l, i, all) => l !== "" || (i > 0 && all[i - 1] !== ""))
        .map((l, i) => {
          const key = `${i}:${l}`;
          if (/^#{1,6}\s/.test(l)) {
            return (
              <div key={key} className="mt-1 font-medium first:mt-0">
                {plain(l.replace(/^#+\s*/, ""))}
              </div>
            );
          }
          const bullet = /^\s*[-*]\s+(.*)$/.exec(l);
          if (bullet) {
            return (
              <div key={key} className="flex gap-2 text-muted-foreground">
                <span className="text-faint">•</span>
                <span className="min-w-0 break-words">{plain(bullet[1])}</span>
              </div>
            );
          }
          if (l === "") return <div key={key} className="h-1" />;
          return (
            <div key={key} className="break-words text-muted-foreground">
              {plain(l)}
            </div>
          );
        })}
    </div>
  );
}
