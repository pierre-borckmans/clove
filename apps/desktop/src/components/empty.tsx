import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function Empty({
  icon,
  title,
  children,
  action,
  className,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "clove-dots relative flex flex-col items-center justify-center overflow-hidden px-6 py-14 text-center animate-in fade-in-0 duration-300",
        className,
      )}
    >
      <div
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 left-1/2 size-48 -translate-x-1/2 -translate-y-1/2 rounded-full bg-bottle/10 blur-3xl dark:bg-bottle/12"
      />
      <div className="relative mb-4 flex size-12 items-center justify-center rounded-2xl border border-border/90 bg-card text-bottle shadow-[var(--card-shadow)] [&_svg]:size-5">
        {icon}
      </div>
      <h3 className="relative text-[14.5px] font-semibold tracking-tight">{title}</h3>
      {children && (
        <p className="relative mt-1.5 max-w-[360px] text-[13px] leading-relaxed text-muted-foreground">{children}</p>
      )}
      {action && <div className="relative mt-5">{action}</div>}
    </div>
  );
}
