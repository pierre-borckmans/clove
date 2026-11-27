import { type ReactNode, type RefObject, useEffect, useState } from "react";
import { cn } from "@/lib/utils";

type Entry = { id: string; title: string; group: string; icon: string };

/** Marks a section for SectionNav: spread on the section element. `icon` is a key the nav's `iconOf` turns into an icon. */
export function tocProps(title: string, group = "", icon = "") {
  const id = `section-${group ? `${group}-` : ""}${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
  return { id, "data-toc": title, "data-toc-group": group, "data-toc-icon": icon };
}

/**
 * A sticky list of a page's sections (those marked with tocProps) that jumps
 * to one on click and follows the one being read. It reads the sections from
 * the page, so dynamic ones (a repo's config) show up as they render.
 */
export function SectionNav({
  scroller,
  groupLabel,
  iconOf,
}: {
  /** The page's scrolling element. */
  scroller: RefObject<HTMLElement | null>;
  /** A heading for each group, by group name (ungrouped entries go first). */
  groupLabel?: (group: string) => ReactNode;
  /** The icon for a section's icon key (see tocProps). */
  iconOf?: (key: string) => ReactNode;
}) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [active, setActive] = useState<string | null>(null);

  useEffect(() => {
    const root = scroller.current;
    if (!root) return;
    const sections = () => [...root.querySelectorAll<HTMLElement>("[data-toc]")];
    const collect = () => {
      const next = sections().map((el) => ({
        id: el.id,
        title: el.dataset.toc ?? "",
        group: el.dataset.tocGroup ?? "",
        icon: el.dataset.tocIcon ?? "",
      }));
      setEntries((prev) =>
        prev.length === next.length &&
        prev.every((e, i) => e.id === next[i].id && e.title === next[i].title && e.icon === next[i].icon)
          ? prev
          : next,
      );
    };
    // The one being read: the last whose top has passed a line near the top;
    // at the very bottom, the last one.
    const follow = () => {
      const list = sections();
      if (!list.length) return;
      const top = root.getBoundingClientRect().top + 96;
      const atEnd = root.scrollTop + root.clientHeight >= root.scrollHeight - 4;
      let current = list[0];
      for (const el of list) if (el.getBoundingClientRect().top <= top) current = el;
      setActive(atEnd ? list[list.length - 1].id : current.id);
    };
    collect();
    follow();
    const observer = new MutationObserver(() => {
      collect();
      follow();
    });
    observer.observe(root, { childList: true, subtree: true });
    root.addEventListener("scroll", follow, { passive: true });
    return () => {
      observer.disconnect();
      root.removeEventListener("scroll", follow);
    };
  }, [scroller]);

  if (entries.length < 2) return null;
  let lastGroup = "";
  return (
    <nav aria-label="Sections" className="flex flex-col gap-0.5 text-[12.5px]">
      {entries.map((entry) => {
        const heading = entry.group && entry.group !== lastGroup ? groupLabel?.(entry.group) : null;
        lastGroup = entry.group;
        return (
          <div key={entry.id} className="flex flex-col">
            {heading && <div className="mt-3 mb-1 px-2.5 text-[11px] text-faint">{heading}</div>}
            <a
              href={`#${entry.id}`}
              aria-current={active === entry.id ? "location" : undefined}
              onClick={(event) => {
                // Scroll the page, not the hash route.
                event.preventDefault();
                document.getElementById(entry.id)?.scrollIntoView({ behavior: "smooth", block: "start" });
              }}
              className={cn(
                "flex items-center gap-2 rounded-md px-2.5 py-1 text-muted-foreground transition-colors hover:bg-foreground/[0.045] hover:text-foreground [&_svg]:size-3.5 [&_svg]:shrink-0",
                active === entry.id && "bg-bottle-soft font-medium text-bottle hover:bg-bottle-soft hover:text-bottle",
              )}
            >
              {entry.icon && iconOf?.(entry.icon)}
              {entry.title}
            </a>
          </div>
        );
      })}
    </nav>
  );
}
