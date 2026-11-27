import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { isMac } from "@/lib/platform";
import { cn } from "@/lib/utils";

const SYMBOLS: Record<string, string> = {
  mod: isMac ? "⌘" : "Ctrl",
  shift: isMac ? "⇧" : "Shift",
  alt: isMac ? "⌥" : "Alt",
  enter: "↵",
  backspace: "⌫",
  up: "↑",
  down: "↓",
  esc: "esc",
};

/** `keys="mod+n"` renders ⌘ N (Ctrl N on Linux) as small key caps. */
export function Shortcut({ keys, className }: { keys: string; className?: string }) {
  return (
    <KbdGroup className={className}>
      {keys.split("+").map((k) => (
        <Kbd
          key={k}
          className={cn(
            "h-[18px] min-w-[18px] rounded-[4.5px] border border-border/90 bg-card px-1 font-sans text-[10.5px] font-medium text-muted-foreground shadow-[var(--kbd-shadow)]",
          )}
        >
          {SYMBOLS[k] ?? k.toUpperCase()}
        </Kbd>
      ))}
    </KbdGroup>
  );
}
