import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { cn } from "@/lib/utils";

const SYMBOLS: Record<string, string> = {
  mod: "⌘",
  shift: "⇧",
  alt: "⌥",
  enter: "↵",
  backspace: "⌫",
  up: "↑",
  down: "↓",
  esc: "esc",
};

/** `keys="mod+n"` renders ⌘ N as small key caps. */
export function Shortcut({ keys, className }: { keys: string; className?: string }) {
  return (
    <KbdGroup className={className}>
      {keys.split("+").map((k) => (
        <Kbd
          key={k}
          className={cn(
            "h-[18px] min-w-[18px] rounded-[4px] border border-border/70 bg-background/60 px-1 font-sans text-[11px] text-muted-foreground shadow-none",
          )}
        >
          {SYMBOLS[k] ?? k.toUpperCase()}
        </Kbd>
      ))}
    </KbdGroup>
  );
}
