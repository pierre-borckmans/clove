import { cn } from "@/lib/utils";

export type DotState = "mounted" | "unmounted" | "warn" | "busy";

export function StatusDot({ state, className }: { state: DotState; className?: string }) {
  return (
    <span className={cn("relative inline-flex size-2 shrink-0", className)} aria-hidden="true">
      {state === "busy" && <span className="absolute inset-0 animate-ping rounded-full bg-bottle opacity-40" />}
      <span
        className={cn(
          "relative inline-flex size-2 rounded-full",
          state === "mounted" && "bg-sage shadow-[0_0_0_3px_var(--sage-soft)]",
          state === "unmounted" && "border-[1.5px] border-faint bg-transparent",
          state === "warn" && "bg-amber shadow-[0_0_0_3px_color-mix(in_oklab,var(--amber)_18%,transparent)]",
          state === "busy" && "bg-bottle",
        )}
      />
    </span>
  );
}
