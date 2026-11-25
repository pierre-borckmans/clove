import {
  accent,
  CopyCommand,
  dim,
  INSTALL,
  ok,
  Prompt,
  REPO,
  SkipLine,
  StepLine,
  Term,
} from "./components";

const WT = "~/src/mono.clove/feat-login/mono";

/** The replay is the real CLI output for `clove new`, with one of five measured
 *  runs on the monorepo (release build). Without JS it shows the finished run. */
function Replay() {
  return (
    <Term
      title="~/src/mono"
      data-replay=""
      aside={
        <span className="flex items-center gap-3">
          <span data-clock className="tabular-nums" />
          <button
            type="button"
            data-replay-button
            hidden
            className="cursor-pointer text-term-dim underline-offset-2 hover:text-term-ink hover:underline"
          >
            replay
          </button>
        </span>
      }
    >
      <span className="term-line" data-r="cmd">
        <Prompt cwd="~/src/mono" />
        <span data-cmd>clove new feat/login</span>
        {"\n"}
      </span>
      <span className="term-line" data-r="show">
        <span className={ok}>+</span> <b className="font-semibold">feat/login</b>
        {"\n"}
      </span>
      <StepLine step="clone image" ms={0} />
      <StepLine step="mount" ms={700} />
      <SkipLine step="fetch" reason="fetched 1m ago (--fetch to force)" />
      <StepLine step="checkout" ms={210} />
      <StepLine step="setup mise" ms={20} />
      <StepLine step="setup direnv-allow" ms={20} />
      <SkipLine step="setup direnv" reason="unchanged since golden" />
      <SkipLine step="setup pnpm" reason="unchanged since golden" />
      <span className="term-line" data-r="show">
        {"\n"}
        <span className={ok}>●</span> <b className="font-semibold">feat-login</b>{" "}
        <span className={dim}>ready in 1.0s</span> {WT}
        {"\n\n"}
      </span>
      <span className="term-line" data-r="cmd">
        <Prompt cwd={WT} />
        <span data-cmd>git branch --show-current</span>
        {"\n"}
      </span>
      <span className="term-line" data-r="show">
        <span className={accent}>feat/login</span>
        {"\n"}
      </span>
      <span className="term-line" data-r="end">
        <Prompt cwd={WT} />
        <span className="term-cursor" />
      </span>
    </Term>
  );
}

export function Hero() {
  return (
    <header className="mx-auto max-w-[74rem] px-4 pt-10 pb-16 sm:px-8 sm:pt-16 sm:pb-24">
      <p className="font-mono text-[0.75rem] tracking-wide text-bottle">
        copy-on-write git worktrees · macOS
      </p>
      <h1 className="mt-5 max-w-[20ch] text-[3.1rem] leading-[0.98] font-medium tracking-[-0.025em] sm:text-[4.6rem] lg:text-[5.6rem]">
        A ready git worktree <br className="hidden sm:inline" />
        in <em className="font-normal text-bottle">about a second.</em>
      </h1>
      <div className="mt-12 grid grid-cols-[minmax(0,1fr)] items-start gap-10 lg:mt-16 lg:grid-cols-[minmax(0,5fr)_minmax(0,6fr)] lg:gap-14">
        <div className="prose-body text-[1.2rem] text-ink-2 sm:text-[1.3rem]">
          <p>
            <code>git worktree add</code> gives you the tracked files and nothing else. clove gives
            you the files plus <code>node_modules</code>, the warmed dev shell and the build caches,
            by cloning one prepared disk image.
          </p>
          <p>On the monorepo I work in, a new worktree went from 72 seconds to about one.</p>
          <CopyCommand cmd={INSTALL} wrap className="mt-8" />
          <p className="mt-4! text-[0.95rem] text-muted">
            macOS only. Not on Homebrew yet.{" "}
            <a href={REPO} className="text-ink!">
              The source is on GitHub
            </a>
            , private while I finish it.
          </p>
        </div>
        <div className="min-w-0">
          <Replay />
          <p className="prose-body mt-3 text-[0.9rem] leading-snug text-muted!">
            One of five runs on that monorepo, release build. Each took 0.93 to 1.04s. The fetch was
            skipped because the repo had been fetched a minute earlier. When it runs, it takes about
            2s, in parallel with the mount.
          </p>
        </div>
      </div>
    </header>
  );
}
