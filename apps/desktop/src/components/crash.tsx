import type { ErrorComponentProps } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";

/** A page threw while rendering: say what, keep the window draggable, offer a reload. */
export function Crash({ error, reset }: ErrorComponentProps) {
  const message = error instanceof Error ? error.message : String(error);
  const stack = error instanceof Error ? error.stack : undefined;
  return (
    <div className="flex h-full flex-col gap-4 overflow-auto p-8 pt-14" data-tauri-drag-region>
      <div>
        <h1 className="font-medium text-[15px]">This screen hit an error</h1>
        <p className="mt-1 text-[13px] text-muted-foreground">
          The rest of clove still works. Reload, or copy the details into an issue.
        </p>
      </div>
      <pre className="select-text whitespace-pre-wrap rounded-md border bg-muted/40 p-3 font-mono text-[12px] text-destructive">
        {message}
      </pre>
      {stack && (
        <pre className="select-text whitespace-pre-wrap rounded-md border bg-muted/20 p-3 font-mono text-[11px] text-muted-foreground">
          {stack}
        </pre>
      )}
      <div className="flex gap-2">
        <Button
          onClick={() => {
            reset();
            window.location.reload();
          }}
        >
          Reload
        </Button>
        <Button variant="outline" onClick={() => navigator.clipboard?.writeText(`${message}\n\n${stack ?? ""}`)}>
          Copy details
        </Button>
      </div>
    </div>
  );
}
