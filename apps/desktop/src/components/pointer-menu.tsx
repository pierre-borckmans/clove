import type { ReactNode } from "react";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

/**
 * A dropdown menu opened at a screen point, for right-click menus: the same
 * items as a row's ••• menu, anchored where the pointer was. A zero-size fixed
 * anchor stands in for the trigger, so it takes no room in the row's layout.
 */
export function PointerMenu({
  at,
  onClose,
  children,
}: {
  at: { x: number; y: number };
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <DropdownMenu open onOpenChange={(open) => !open && onClose()}>
      <DropdownMenuTrigger asChild>
        <span aria-hidden="true" className="pointer-events-none fixed size-0" style={{ left: at.x, top: at.y }} />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        sideOffset={2}
        className="w-60"
        // The menu renders in a portal, but React still bubbles its events to
        // the row it belongs to: stop them, or the row would take focus away
        // from the menu, or act on the click itself.
        onMouseDown={(event) => event.stopPropagation()}
        onClick={(event) => event.stopPropagation()}
        onDoubleClick={(event) => event.stopPropagation()}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
      >
        {children}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
