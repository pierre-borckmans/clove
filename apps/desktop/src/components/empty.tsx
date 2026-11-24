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
        "flex flex-col items-center justify-center px-6 py-14 text-center animate-in fade-in-0 duration-300",
        className,
      )}
    >
      <div className="mb-4 flex size-11 items-center justify-center rounded-xl border bg-card text-muted-foreground shadow-xs [&_svg]:size-5">
        {icon}
      </div>
      <h3 className="text-[14px] font-semibold tracking-tight">{title}</h3>
      {children && <p className="mt-1.5 max-w-[340px] text-[13px] leading-relaxed text-muted-foreground">{children}</p>}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}
