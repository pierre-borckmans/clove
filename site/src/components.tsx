export const REPO = "https://github.com/pierre-borckmans/clove";
export const INSTALL = "brew install pierre-borckmans/tap/clove";

/** A command with a copy button. The button only appears once JS runs. */
export function CopyCommand({
  cmd,
  wrap = false,
  className = "",
}: {
  cmd: string;
  wrap?: boolean;
  className?: string;
}) {
  return (
    <div
      className={`flex items-stretch overflow-hidden rounded-md border border-rule bg-[rgba(11,16,13,0.85)] font-mono backdrop-blur text-[0.8rem] ${className}`}
    >
      <span className="select-none py-3 pr-2 pl-4 text-bottle" aria-hidden="true">
        $
      </span>
      <code
        className={`raw min-w-0 flex-1 py-3 pr-4 ${wrap ? "[overflow-wrap:anywhere]" : "overflow-x-auto whitespace-nowrap"}`}
      >
        {cmd}
      </code>
      <button
        type="button"
        data-copy={cmd}
        hidden
        className="cursor-pointer border-l border-rule px-3 text-muted hover:text-ink"
      >
        copy
      </button>
    </div>
  );
}
