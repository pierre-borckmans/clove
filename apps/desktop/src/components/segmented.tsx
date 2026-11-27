import { type CSSProperties, type ReactNode, useLayoutEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

export type SegmentedOption<T extends string> = { value: T; label: string; icon?: ReactNode };

/**
 * A row of mutually exclusive buttons with a highlight that slides to the
 * picked one. `transitionName` names the highlight for view transitions, so it
 * also slides when the switch runs inside one (styles.css); it must be unique
 * on screen.
 */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
  transitionName,
}: {
  label: string;
  value: T;
  options: SegmentedOption<T>[];
  onChange: (value: T) => void;
  transitionName?: string;
}) {
  const buttons = useRef(new Map<T, HTMLButtonElement>());
  const [box, setBox] = useState<{ x: number; w: number } | null>(null);

  // Measure the picked option, and again if it resizes (a font arriving late).
  useLayoutEffect(() => {
    const el = buttons.current.get(value);
    if (!el) return;
    const measure = () => setBox({ x: el.offsetLeft, w: el.offsetWidth });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [value]);

  return (
    <fieldset
      aria-label={label}
      className="relative flex shrink-0 items-center gap-0.5 rounded-md border bg-muted/50 p-0.5"
    >
      {box && (
        <span
          aria-hidden="true"
          className="segmented-highlight absolute top-0.5 bottom-0.5 left-0 rounded-[5px] bg-bottle-soft shadow-sm ring-1 ring-bottle/30"
          style={
            { width: box.w, transform: `translateX(${box.x}px)`, viewTransitionName: transitionName } as CSSProperties
          }
        />
      )}
      {options.map((option) => (
        <button
          key={option.value}
          ref={(el) => {
            if (el) buttons.current.set(option.value, el);
            else buttons.current.delete(option.value);
          }}
          type="button"
          aria-pressed={value === option.value}
          onClick={() => value !== option.value && onChange(option.value)}
          className={cn(
            "relative flex h-6 items-center gap-1.5 rounded-[5px] px-2 text-[11.5px] font-medium text-muted-foreground transition-colors hover:text-foreground [&_svg]:size-3.5",
            "aria-pressed:text-bottle aria-pressed:hover:text-bottle",
          )}
        >
          {option.icon}
          {option.label}
        </button>
      ))}
    </fieldset>
  );
}
