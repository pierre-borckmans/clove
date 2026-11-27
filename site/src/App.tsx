import type { ReactNode } from "react";
import { CopyCommand, INSTALL } from "./components";
import { Backdrop, FooterLinks, Nav } from "./layout";
import { VideoDialog, WatchButton } from "./video";
import {
  Clash,
  Cost,
  Detect,
  Empty,
  Fresh,
  Golden,
  Hacks,
  Internals,
  Kicker,
  L,
  Mark,
  Prompt,
  Race,
  Term,
  Window,
  Worktrees,
} from "./visuals";

function Beat({
  id,
  kicker,
  title,
  children,
  copy,
  kickerColor,
}: {
  id: string;
  kicker: string;
  title: ReactNode;
  copy?: ReactNode;
  children: ReactNode;
  kickerColor?: string;
}) {
  return (
    <section id={id} className="mx-auto max-w-[76rem] px-5 py-20 sm:px-8 sm:py-28">
      <div data-reveal>
        <Kicker color={kickerColor}>{kicker}</Kicker>
      </div>
      <h2
        data-reveal
        style={{ "--d": "80ms" } as React.CSSProperties}
        className="mt-4 max-w-[22ch] text-[2.3rem] leading-[1.05] font-normal tracking-[-0.02em] text-balance sm:text-[3.4rem]"
      >
        {title}
      </h2>
      {copy && (
        <p
          data-reveal
          style={{ "--d": "160ms" } as React.CSSProperties}
          className="mt-5 max-w-[40rem] text-[1.2rem] leading-[1.55] text-ink-2"
        >
          {copy}
        </p>
      )}
      <div className="mt-12">{children}</div>
    </section>
  );
}

const em = (s: string) => <i className="text-bottle">{s}</i>;

function Hero() {
  return (
    <header className="mx-auto max-w-[76rem] px-5 pt-14 pb-10 sm:px-8 sm:pt-24">
      <div data-reveal>
        <Kicker>copy-on-write git worktrees · macOS and Linux</Kicker>
      </div>
      <h1
        data-reveal
        style={{ "--d": "80ms" } as React.CSSProperties}
        className="mt-6 max-w-[14ch] text-[3.4rem] leading-[0.98] font-normal tracking-[-0.03em] sm:text-[5.6rem]"
      >
        Worktrees, {em("ready in a second.")}
      </h1>
      <p
        data-reveal
        style={{ "--d": "160ms" } as React.CSSProperties}
        className="mt-7 max-w-[38rem] text-[1.3rem] leading-[1.5] text-ink-2"
      >
        Dependencies, dev shell and local files: already there in every new git worktree. For you,
        and every agent you run.
      </p>
      <div
        data-reveal
        style={{ "--d": "240ms" } as React.CSSProperties}
        className="mt-9 flex max-w-[50rem] flex-wrap items-center gap-3"
      >
        <div className="min-w-[20rem] flex-1">
          <CopyCommand cmd={INSTALL} />
        </div>
        <WatchButton />
      </div>
      <div data-reveal style={{ "--d": "300ms" } as React.CSSProperties} className="mt-14">
        <Race />
      </div>
    </header>
  );
}

function Surfaces() {
  return (
    <Beat
      id="surfaces"
      kicker="three ways in"
      title={<>Command line, terminal UI, {em("desktop app.")}</>}
      copy="The same features everywhere, on macOS and Linux. Every command also speaks --json, for scripts and agents."
    >
      <div className="grid gap-6">
        <Window
          title="clove · full-screen terminal UI"
          src="/shots/tui-main.png"
          alt="The clove terminal UI listing clones with their branch, changes, age and size"
        />
        <div className="grid gap-6 lg:grid-cols-[1.4fr_1fr]">
          <Window
            title="clove · desktop app"
            src="/shots/app-repo.png"
            alt="The clove desktop app showing a repository's golden image and clones"
          />
          <Term title="~/src/mono">
            <L>
              <Prompt cmd="clove" args="ls" />
            </L>
            <L at={200} c="var(--muted)">
              {"NAME          CHANGES   OWN"}
            </L>
            <L at={300}>{"feat-login    modified   48M"}</L>
            <L at={400}>{"fix-race      clean      96M"}</L>
            <L at={500}>{"pr-1432       clean      12M"}</L>
            <L at={800}>
              <Prompt cmd="clove" args="new pr:1441 -x claude" />
            </L>
            <L at={1100} c="var(--bottle)">
              ● pr-1441 ready in 0.9s · starting claude
            </L>
          </Term>
        </div>
      </div>
    </Beat>
  );
}

function Feature({
  kicker,
  title,
  copy,
  children,
  flip = false,
}: {
  kicker: string;
  title: ReactNode;
  copy: string;
  children: ReactNode;
  flip?: boolean;
}) {
  return (
    <div className="grid items-center gap-10 py-10 lg:grid-cols-[1fr_1.4fr]">
      <div className={flip ? "lg:order-2" : ""}>
        <div data-reveal>
          <Kicker>{kicker}</Kicker>
        </div>
        <h3
          data-reveal
          style={{ "--d": "80ms" } as React.CSSProperties}
          className="mt-4 text-[2.1rem] leading-[1.1] font-normal tracking-[-0.015em]"
        >
          {title}
        </h3>
        <p
          data-reveal
          style={{ "--d": "160ms" } as React.CSSProperties}
          className="mt-4 text-[1.1rem] leading-[1.55] text-ink-2"
        >
          {copy}
        </p>
      </div>
      <div className={flip ? "lg:order-1" : ""}>{children}</div>
    </div>
  );
}

function Features() {
  return (
    <section id="features" className="mx-auto max-w-[76rem] px-5 py-16 sm:px-8">
      <Feature
        kicker="clove update"
        title={<>Up to date, with {em("your agent")} resolving the conflicts.</>}
        copy="Rebases a clone onto the latest base and reruns only the setup steps that changed. When the rebase stops on conflicts, your coding agent resolves them in the clone, from the CLI, the TUI or the app. Claude by default; Codex, or any command, through update.resolver."
      >
        <Window
          title="clove · update"
          src="/shots/tui-conflict.png"
          alt="The clove TUI offering to resolve rebase conflicts with Claude"
        />
      </Feature>
      <Feature
        flip
        kicker="clove prune · clove log"
        title={<>Merged pull requests, {em("cleaned up.")}</>}
        copy="Clones whose pull request merged, squash merges included, are removed by hand or automatically. A history keeps every removal, so any branch can come back."
      >
        <Window
          title="clove · history"
          src="/shots/app-history.png"
          alt="The clove app's history of pruned clones"
        />
      </Feature>
      <Feature
        kicker="clove agents install"
        title={<>Your {em("agents")} reach for clove on their own.</>}
        copy="One command teaches Claude Code, Codex, OpenCode and Pi to use clones. In repos clove manages, Claude Code and OpenCode use clones for their own worktrees."
      >
        <Term title="~/src/mono">
          <L>
            <Prompt cmd="clove" args="agents install" />
          </L>
          <L at={250} c="var(--bottle)">
            ✓ Claude Code worktree hooks
          </L>
          <L at={400} c="var(--bottle)">
            ✓ Codex instructions
          </L>
          <L at={550} c="var(--bottle)">
            ✓ OpenCode plugin
          </L>
          <L at={700} c="var(--bottle)">
            ✓ Pi extension
          </L>
        </Term>
      </Feature>
    </section>
  );
}

function Outro() {
  return (
    <footer className="mx-auto max-w-[76rem] px-5 pt-24 pb-16 text-center sm:px-8">
      <div
        data-reveal
        className="flex items-center justify-center gap-4 text-[4rem] tracking-[-0.03em] sm:text-[6rem]"
      >
        <Mark size={90} /> clove
      </div>
      <p
        data-reveal
        style={{ "--d": "100ms" } as React.CSSProperties}
        className="mt-2 text-[1.6rem] text-muted"
      >
        Worktrees, {em("ready in a second.")}
      </p>
      <div
        data-reveal
        style={{ "--d": "200ms" } as React.CSSProperties}
        className="mx-auto mt-10 max-w-[34rem] text-left"
      >
        <CopyCommand cmd={INSTALL} />
      </div>
      <div
        data-reveal
        style={{ "--d": "260ms" } as React.CSSProperties}
        className="mt-5 flex justify-center gap-2 font-mono text-[0.8rem] text-muted"
      >
        <span className="rounded-full border border-rule px-3 py-1">macOS</span>
        <span className="rounded-full border border-rule px-3 py-1">Linux</span>
      </div>
      <div className="mt-16">
        <FooterLinks />
      </div>
    </footer>
  );
}

/** The landing page's sections, for the rail on the right. */
const RAIL: [string, string][] = [
  ["clash", "the problem"],
  ["worktrees", "worktrees"],
  ["empty", "but empty"],
  ["hacks", "the hacks"],
  ["cost", "the cost"],
  ["golden", "golden image"],
  ["detect", "detection"],
  ["fresh", "always warm"],
  ["internals", "under the hood"],
  ["surfaces", "three ways in"],
  ["features", "features"],
];

/** A quiet rail of dots: where you are on the page. Labels show on hover. */
function Rail() {
  return (
    <nav className="rail" aria-label="Sections">
      {RAIL.map(([id, label]) => (
        <a key={id} href={`#${id}`} data-toc={id} className="rail-dot">
          <span className="rail-label">{label}</span>
        </a>
      ))}
    </nav>
  );
}

export function App() {
  return (
    <>
      <Backdrop />
      <Nav />
      <Hero />
      <Rail />
      <VideoDialog />
      <main>
        <Beat
          id="clash"
          kicker="the problem"
          kickerColor="var(--red)"
          title={<>A few agents, {em("one checkout.")}</>}
          copy="Every agent works on its own branch. In one checkout, one switches branches under another, and your login fix lands on another agent's branch."
        >
          <Clash />
        </Beat>
        <Beat
          id="worktrees"
          kicker="the fix git already has"
          title={<>One worktree per branch, {em("one folder each.")}</>}
          copy="git worktree add gives every branch its own folder, all sharing one repository. Each agent works in its own, and nobody switches branches under anyone. A new one takes about a second."
        >
          <Worktrees />
        </Beat>
        <Beat
          id="empty"
          kicker="but"
          kickerColor="var(--red)"
          title="git worktree add starts empty."
          copy="A fresh worktree has the tracked files and nothing else: no dependencies, no dev shell, and none of your local files."
        >
          <Empty />
        </Beat>
        <Beat
          id="hacks"
          kicker="so your agent improvises"
          kickerColor="var(--peel)"
          title={<>Reinstall everything, or {em("hack around it.")}</>}
          copy="It reinstalls in every worktree, or gives up and reaches back into your main checkout. And the next agent rediscovers all of it, from scratch."
        >
          <Hacks />
        </Beat>
        <Beat
          id="cost"
          kicker="every new worktree"
          kickerColor="var(--peel)"
          title="Over a minute and gigabytes of disk, each time."
          copy="Measured on a monorepo with ~35k files, ~4.5k npm packages and a nix flake dev shell. Your laptop fan has opinions."
        >
          <Cost />
        </Beat>
        <Beat
          id="golden"
          kicker="how it works"
          title={<>One golden image. {em("Copy‑on‑write")} clones of it.</>}
          copy="The golden image is one worktree of your base branch, with everything installed and warmed. Each new worktree is a clone of it that shares every block until it changes, registered as a regular git worktree."
        >
          <Golden />
        </Beat>
        <Beat
          id="detect"
          kicker="detected, then configurable"
          title={<>Your toolchain, {em("found on its own.")}</>}
          copy="mise, direnv and nix, pnpm, bun, yarn, npm, uv and cargo, plus your local files. Anything else is a step in .clove.toml, shared with your team."
        >
          <Detect />
        </Beat>
        <Beat
          id="fresh"
          kicker="always warm"
          title={<>Stays warm {em("on its own.")}</>}
          copy="Each time you pull, a background service refreshes the golden image, rerunning only the steps whose inputs changed. Existing clones keep working, untouched."
        >
          <Fresh />
        </Beat>
        <Beat
          id="internals"
          kicker="under the hood"
          title={<>What your file system {em("already does well.")}</>}
          copy="On macOS, each clone is an APFS disk image copied whole by a single clonefile call. On Linux, it's a btrfs snapshot, taken in a few milliseconds."
        >
          <Internals />
        </Beat>
        <Surfaces />
        <Features />
      </main>
      <Outro />
    </>
  );
}
