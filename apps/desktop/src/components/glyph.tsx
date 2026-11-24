import { cn } from "@/lib/utils";

/** The clove mark: a bud seen from above, four sepals around it. */
export function Glyph({ className }: { className?: string }) {
  const sepal = "M50 4 C58 18 60 28 56.5 37 L43.5 37 C40 28 42 18 50 4 Z";
  return (
    <svg viewBox="0 0 100 100" className={cn("size-4", className)} aria-hidden="true">
      <g fill="currentColor">
        {[0, 90, 180, 270].map((r) => (
          <path key={r} d={sepal} transform={`rotate(${r} 50 50)`} />
        ))}
        <circle cx="50" cy="50" r="11" />
      </g>
    </svg>
  );
}
