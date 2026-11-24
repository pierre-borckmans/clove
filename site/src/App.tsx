import type { ReactNode } from "react";
import {
  CopyCommand,
  INSTALL,
  Json,
  ok,
  Prompt,
  Prose,
  REPO,
  Section,
  Term,
  Toml,
} from "./components";
import { Hero } from "./Hero";
import { Blocks, Pipeline } from "./Mechanism";

function Nav() {
  const links = [
    ["#how", "How it works"],
    ["#numbers", "Numbers"],
    ["#cli", "CLI"],
    ["#agents", "Agents"],
    ["#limits", "Limits"],
  ];
  return (
    <nav className="mx-auto flex max-w-[74rem] items-center justify-between gap-6 px-4 py-5 sm:px-8">
      <a href="#top" className="flex items-center gap-2 text-[1.35rem] font-semibold no-underline">
        <svg viewBox="0 0 20 20" className="size-5" aria-hidden="true">
          <rect
            x="2"
            y="3"
            width="10"
            height="13"
            rx="1.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
          />
          <rect
            x="7"
            y="5"
            width="10"
            height="13"
            rx="1.5"
            fill="var(--rust)"
            stroke="var(--rust)"
            strokeWidth="1.6"
          />
        </svg>
        clove
      </a>
      <div className="flex items-center gap-5 font-mono text-[0.75rem] text-ink-2">
        {links.map(([href, label]) => (
          <a key={href} href={href} className="hidden no-underline hover:text-rust md:inline">
            {label}
          </a>
        ))}
        <a href={REPO} className="no-underline hover:text-rust">
          GitHub
        </a>
        <button
          type="button"
          data-theme-toggle
          hidden
          aria-label="Toggle dark mode"
          className="cursor-pointer rounded border border-rule px-2 py-1 hover:border-ink"
        >
          <span className="dark-label">dark</span>
        </button>
      </div>
    </nav>
  );
}

function Problem() {
  const facts: [string, string][] = [
    ["tracked files", "35k"],
    ["npm packages", "4,470"],
    ["dev shell", "nix flake, via direnv"],
    ["nix/direnv eval", "41s"],
    ["pnpm install --offline", "30s"],
    ["before I could work", "~72s"],
  ];
  return (
    <Section id="problem" label="the problem" title="A fresh worktree is an empty one.">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-12 xl:grid-cols-[minmax(0,1fr)_20rem]">
        <Prose>
          <p>
            Worktrees are the obvious way to work on several branches at once, or to give each
            coding agent its own checkout. The catch is that <code>git worktree add</code> only
            writes tracked files. No <code>node_modules</code>, no direnv or nix shell, no mise
            tools, no build caches.
          </p>
          <p>
            On the monorepo I work in, every new worktree cost me about 72 seconds before I could
            run anything: 41s for nix to evaluate the dev shell and 30s for{" "}
            <code>pnpm install --offline</code>, with the store already warm. Each one also wrote a
            few GB of files.
          </p>
          <h3 className="mt-10 text-[1.4rem] font-medium text-ink">
            Copying per file saves disk, not time
          </h3>
          <p className="mt-3!">
            The existing fixes, like worktrunk's copy-ignored, <code>cp -c</code> and git-cow, copy
            the ignored files from your main checkout one by one, with reflinks. Disk use stays low.
            Time still grows with the file count. Here, 549k files took minutes, and{" "}
            <code>clonefile</code> on <code>node_modules</code> alone took 15s.
          </p>
        </Prose>
        <dl className="self-start border-t border-ink font-mono text-[0.8rem]">
          <div className="border-b border-rule py-2 text-[0.7rem] text-muted">the monorepo</div>
          {facts.map(([k, v], i) => (
            <div
              key={k}
              className={`flex justify-between gap-4 border-b py-2 ${i === facts.length - 1 ? "border-ink" : "border-rule"}`}
            >
              <dt className="text-ink-2">{k}</dt>
              <dd className={i === facts.length - 1 ? "font-semibold text-rust" : ""}>{v}</dd>
            </div>
          ))}
        </dl>
      </div>
    </Section>
  );
}

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <li className="grid grid-cols-[2rem_minmax(0,1fr)] gap-x-3">
      <span className="pt-1 font-mono text-[0.8rem] text-rust">{n}</span>
      <div>
        <h3 className="text-[1.2rem] font-medium text-ink">{title}</h3>
        <div className="prose-body mt-1 text-ink-2">{children}</div>
      </div>
    </li>
  );
}

function How() {
  const checks = [
    "Adding a dependency in one clone leaves the others alone.",
    "Editing the nix flake in one clone leaves the other clones' shells alone.",
    "File watchers (Vite, tsgo) see edits inside a clone.",
    "Docker bind mounts from a clone work (tested with OrbStack).",
    "Go and Rust builds work inside a clone.",
  ];
  return (
    <Section id="how" label="how it works" title="Clone one file, not 549k of them.">
      <Prose>
        <p>
          clove keeps one golden disk image per repo: an APFS volume with the base branch checked
          out, dependencies installed and the dev environment warmed. A new worktree is a copy of
          that single file. APFS <code>clonefile()</code> copies it in constant time, whatever is
          inside.
        </p>
      </Prose>
      <div className="mt-10">
        <Pipeline />
      </div>
      <ol className="mt-14 grid max-w-[60rem] gap-x-12 gap-y-8 md:grid-cols-2">
        <Step n={1} title="clove init builds the golden image">
          <p>
            It checks out the base branch inside a fresh image and runs your warm steps there:{" "}
            <code>pnpm install</code>, the direnv or nix eval, <code>mise install</code>. This is
            the slow part, and it runs once.
          </p>
        </Step>
        <Step n={2} title="clove new clones and mounts it">
          <p>
            <code>clonefile()</code> on the image takes 0.02s and uses no extra disk until something
            writes to the clone. Mounting it takes 0.70s.
          </p>
        </Step>
        <Step n={3} title="The mount becomes a real git worktree">
          <p>
            clove registers it with your repo and copies the golden index, so git doesn't rehash 35k
            files on the first <code>git status</code> (0.09s). Then it switches to your branch.
          </p>
        </Step>
        <Step n={4} title="Setup reruns only what changed">
          <p>
            Each setup step lists the files it depends on. If your branch has the same lockfile as
            the golden commit, <code>pnpm install</code> doesn't run at all.
          </p>
        </Step>
      </ol>

      <div className="mt-16 grid gap-10 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] xl:items-start">
        <div>
          <h3 className="text-[1.4rem] font-medium">Copy-on-write, per block</h3>
          <Prose className="mt-3">
            <p>
              Every clone starts as a reference to the golden image's blocks. A block gets its own
              copy only when that clone writes to it. A clone you haven't touched costs no disk at
              all.
            </p>
            <p>
              <code>clove refresh</code> builds the next golden generation from a clone of the
              current one, warms it, and swaps it in atomically. Existing clones keep the blocks
              they have.
            </p>
          </Prose>
        </div>
        <Blocks />
      </div>

      <div className="mt-16">
        <h3 className="text-[1.4rem] font-medium">What I checked</h3>
        <ul className="mt-4 max-w-[44rem] divide-y divide-rule border-y border-rule">
          {checks.map((c) => (
            <li key={c} className="flex gap-3 py-2.5 text-ink-2">
              <span className="font-mono text-sage-ink" aria-hidden="true">
                ✓
              </span>
              {c}
            </li>
          ))}
        </ul>
      </div>
    </Section>
  );
}

function Bar({
  label,
  total,
  segments,
  emphasis = false,
}: {
  label: ReactNode;
  total: string;
  segments: { s: number; name?: string; cls: string }[];
  emphasis?: boolean;
}) {
  const max = 72;
  return (
    <div className="grid grid-cols-[minmax(0,1fr)] gap-2 sm:grid-cols-[15rem_minmax(0,1fr)] sm:items-center sm:gap-6">
      <div className="text-[1rem] leading-tight text-ink-2">{label}</div>
      <div className="flex items-center gap-3">
        <div className="flex h-9 min-w-0 flex-1 items-stretch">
          {segments.map((seg) => (
            <div
              key={seg.name ?? seg.s}
              className={`flex min-w-[3px] items-center overflow-hidden border-r-2 border-paper px-2 font-mono text-[0.7rem] whitespace-nowrap ${seg.cls}`}
              style={{ width: `${(seg.s / max) * 100}%` }}
              title={seg.name ? `${seg.name}: ${seg.s}s` : undefined}
            >
              {seg.name && <span className="truncate">{seg.name}</span>}
            </div>
          ))}
        </div>
        <span
          className={`w-14 shrink-0 text-right font-mono tabular-nums ${emphasis ? "text-[1.25rem] font-semibold text-rust" : "text-[1rem]"}`}
        >
          {total}
        </span>
      </div>
    </div>
  );
}

function Numbers() {
  const rows: [string, string, string][] = [
    ["clone image", "0.02s", "clonefile() of one file"],
    ["mount", "0.70s", "attach the APFS image"],
    ["git switch", "0.21s", "check out your branch"],
    ["git status", "0.09s", "index copied, no rehash"],
    ["direnv", "0.24s", "nix cache hit"],
    ["pnpm", "2.76s", "install check (see below)"],
  ];
  return (
    <Section id="numbers" label="the numbers" title="72 seconds down to about one.">
      <div className="space-y-5">
        <Bar
          label={
            <>
              <code className="font-mono text-[0.85rem]">git worktree add</code>, then setup
            </>
          }
          total="~72s"
          segments={[
            { s: 41, name: "nix/direnv eval 41s", cls: "bg-ink text-paper" },
            { s: 30, name: "pnpm install 30s", cls: "bg-ink-2 text-paper" },
            { s: 1, cls: "bg-muted" },
          ]}
        />
        <Bar
          label="clove, first prototype (bash, pnpm check included)"
          total="~6s"
          segments={[{ s: 6, cls: "bg-sage" }]}
        />
        <Bar
          label="clove, lockfile unchanged"
          total="~1s"
          emphasis
          segments={[{ s: 0.93, cls: "bg-rust" }]}
        />
        <div className="grid grid-cols-[minmax(0,1fr)] sm:grid-cols-[15rem_minmax(0,1fr)] sm:gap-6">
          <div />
          <div className="flex justify-between pr-[4.25rem] font-mono text-[0.68rem] text-muted">
            <span>0s</span>
            <span>36s</span>
            <span>72s</span>
          </div>
        </div>
      </div>

      <div className="mt-16 grid gap-10 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <div>
          <h3 className="text-[1.4rem] font-medium">Where the time goes</h3>
          <table className="mt-4 w-full border-t border-ink font-mono text-[0.8rem]">
            <thead>
              <tr className="border-b border-rule text-left text-[0.7rem] text-muted">
                <th className="py-2 font-normal">step</th>
                <th className="py-2 text-right font-normal">time</th>
                <th className="hidden py-2 pl-6 font-normal sm:table-cell">what happens</th>
              </tr>
            </thead>
            <tbody>
              {rows.map(([step, t, what]) => (
                <tr key={step} className="border-b border-rule">
                  <td className="py-2">{step}</td>
                  <td className="py-2 text-right tabular-nums">{t}</td>
                  <td className="hidden py-2 pl-6 text-ink-2 sm:table-cell">{what}</td>
                </tr>
              ))}
              <tr className="border-b border-ink">
                <td className="py-2 font-semibold">end to end</td>
                <td className="py-2 text-right font-semibold text-rust">~6s</td>
                <td className="hidden py-2 pl-6 text-ink-2 sm:table-cell">bash prototype</td>
              </tr>
            </tbody>
          </table>
        </div>
        <Prose className="xl:pt-12">
          <p>
            These are the per-step numbers from the bash prototype on the monorepo. It ran a pnpm
            install check on every new worktree, which is most of its 6 seconds.
          </p>
          <p>
            The Rust version skips pnpm entirely when the lockfile matches the golden commit, which
            leaves the clone, the mount and the checkout: under a second. In its own test suite{" "}
            <code>clove new</code> takes about 0.6s.
          </p>
          <p>
            When the lockfile did change, pnpm runs as usual with the golden image's{" "}
            <code>node_modules</code> already in place, so it only installs the difference.
          </p>
        </Prose>
      </div>
    </Section>
  );
}

function Cli() {
  const cmds: [string, string][] = [
    ["clove init", "Build the golden image for this repo. Slow, once."],
    ["clove new <branch>", "Clone, mount, check out. Alias: add."],
    ["clove ls", "List clones with branch, changes and age."],
    ["clove cd <clone>", "Jump into a clone. No argument: the main checkout."],
    ["clove run <clone> -- cmd", "Run a command in the clone's dev environment."],
    ["clove rm <clone>", "Unmount, delete the image and the git registration."],
    ["clove refresh", "Build the next golden image from the latest base."],
    ["clove status", "Golden generation, age, commits behind, clones."],
    ["clove doctor", "Check the machine and the repo setup."],
    ["clove gc", "Find leftovers from interrupted runs."],
    ["clove mount --all-repos", "Remount everything after a reboot."],
  ];
  return (
    <Section id="cli" label="the cli" title="A handful of commands, all with --json.">
      <dl className="grid grid-cols-[minmax(0,1fr)] gap-x-12 border-t border-ink md:grid-cols-2">
        {cmds.map(([c, d]) => (
          <div key={c} className="border-b border-rule py-3">
            <dt className="font-mono text-[0.8rem] whitespace-nowrap text-ink">{c}</dt>
            <dd className="mt-0.5 text-[1rem] leading-snug text-ink-2">{d}</dd>
          </div>
        ))}
      </dl>
      <Prose className="mt-5 text-[1rem]">
        <p>
          A terminal UI, <code>clove ui</code>, is in progress and not in the current binary.
        </p>
      </Prose>
      <div className="mt-14 grid grid-cols-[minmax(0,1fr)] gap-10 xl:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
        <div className="prose-body text-[1rem] text-ink-2">
          <h3 className="text-[1.4rem] font-medium text-ink">Shell integration</h3>
          <p className="mt-3">
            It lets <code>clove new</code> and <code>clove cd</code> change your directory, and adds
            completions for branches and clones.
          </p>
          <CopyCommand cmd={'eval "$(clove shell-init zsh)"'} className="mt-4" />
          <p className="mt-2 text-[0.9rem] text-muted!">
            bash works the same way. For fish, pipe it to <code>source</code>.
          </p>
        </div>
        <div className="min-w-0">
          <Term title="~/code/mono">
            <Prompt />
            clove doctor{"\n"}
            {"  "}
            <span className={ok}>✓</span> {"disk images".padEnd(14)} ASIF via diskutil (macOS 26+)
            {"\n  "}
            <span className={ok}>✓</span> {"state volume".padEnd(14)} /Users/you/.clove is on APFS
            {"\n  "}
            <span className={ok}>✓</span> {"git".padEnd(14)} 2.50.1{"\n  "}
            <span className={ok}>✓</span> {"steps".padEnd(14)} direnv, pnpm (from config){"\n  "}
            <span className={ok}>✓</span> {"pnpm store".padEnd(14)} pinned to
            /Users/you/Library/pnpm/store{"\n  "}
            <span className={ok}>✓</span> {"golden".padEnd(14)} gen 3 at 4f1c2a9e07 (12 commits
            behind origin/main)
          </Term>
        </div>
      </div>
    </Section>
  );
}

function Agents() {
  return (
    <Section id="agents" label="agents" title="One clone per agent.">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-12 xl:grid-cols-[minmax(0,5fr)_minmax(0,6fr)]">
        <Prose>
          <p>
            Each agent gets its own volume with its own <code>node_modules</code> and dev shell. One
            agent adding a package or changing the flake doesn't touch the others, and none of them
            wait 72 seconds to start.
          </p>
          <p>
            <code>-x</code> runs a command in the clone once it's ready, inside its direnv or mise
            environment. <code>clove new agent/parser -x claude</code> starts Claude Code there.
          </p>
          <p>
            Every command takes <code>--json</code> and prints machine-readable output on stdout,
            with progress on stderr. clove holds its repo lock only while copying the image, so
            several <code>clove new</code> calls can run at once.
          </p>
        </Prose>
        <div className="min-w-0 space-y-6">
          <Term title="~/code/mono">
            <Prompt />
            {"clove new agent/parser -x claude\n"}
            <Prompt />
            {"clove new agent/cache  -x claude\n"}
            <Prompt />
            {"clove run agent-parser -- pnpm test"}
          </Term>
          <Term title="~/code/mono">
            <Prompt />
            {"clove new agent/parser --json | jq '{path, branch}'\n"}
            <Json
              value={{
                path: "/Users/you/code/mono.clove/agent-parser/mono",
                branch: "agent/parser",
              }}
            />
          </Term>
        </div>
      </div>
    </Section>
  );
}

const TOML = `base = "origin/main"

[[step]]
name = "direnv"
warm = "direnv exec . true"
setup = "direnv exec . true"
watch = ["flake.nix", "flake.lock", ".envrc"]
wrap = false

[[step]]
name = "pnpm"
warm = "pnpm install --frozen-lockfile"
setup = "pnpm install --prefer-offline --frozen-lockfile"
watch = ["pnpm-lock.yaml", "pnpm-workspace.yaml"]`;

function Config() {
  const keys: [string, string][] = [
    ["warm", "Runs in the golden image on init and refresh. Allowed to be slow."],
    ["setup", "Runs in each new clone. Should be incremental."],
    ["watch", "Paths that decide whether setup runs. Unchanged since golden: skipped."],
    ["wrap", "Run inside direnv or mise. Off for steps that set that environment up."],
  ];
  return (
    <Section id="config" label="config" title="Steps: warm once, set up when inputs change.">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-12 xl:grid-cols-[minmax(0,6fr)_minmax(0,5fr)]">
        <div className="min-w-0">
          <Term title=".clove.toml">
            <Toml src={TOML} />
          </Term>
        </div>
        <div>
          <dl className="border-t border-ink">
            {keys.map(([k, d]) => (
              <div
                key={k}
                className="grid grid-cols-[4.5rem_minmax(0,1fr)] gap-x-4 border-b border-rule py-2.5"
              >
                <dt className="font-mono text-[0.8rem] text-rust">{k}</dt>
                <dd className="text-[0.98rem] leading-snug text-ink-2">{d}</dd>
              </div>
            ))}
          </dl>
          <Prose className="mt-6 text-[1rem]">
            <p>
              Without a <code>.clove.toml</code>, clove detects steps from the files in the repo:
              pnpm, npm, yarn, bun, direnv, mise and uv. <code>clove config --write</code> saves
              what it detected so the team can share and edit it.
            </p>
          </Prose>
        </div>
      </div>
    </Section>
  );
}

function Limits() {
  const items: [string, ReactNode][] = [
    [
      "macOS only, for now.",
      <>
        clove needs APFS <code>clonefile()</code> and disk images. Linux support on btrfs and XFS,
        which both have reflinks, is planned.
      </>,
    ],
    [
      "Each clone is a mounted volume.",
      <>
        Mounts don't survive a reboot. <code>clove mount --all-repos</code> brings them all back,
        and <code>clove cd</code> remounts the one you ask for.
      </>,
    ],
    [
      "Absolute paths don't move.",
      <>
        Build outputs that record their own path, like Python venvs or cargo's target fingerprints,
        are valid in the golden image's path but not in a clone's. Those rebuild in each clone.
      </>,
    ],
    [
      "The golden image ages.",
      <>
        Clones start from the golden commit and switch to your branch, so a stale golden image means
        a bigger checkout and more setup. <code>clove refresh</code> brings it up to date, and{" "}
        <code>clove doctor</code> warns past 200 commits behind.
      </>,
    ],
    [
      "Not packaged yet.",
      <>
        <code>brew install clove</code> is the plan. Until then it builds from source with cargo,
        and the repository is private while I finish it.
      </>,
    ],
  ];
  return (
    <Section id="limits" label="limits" title="What it doesn't do yet.">
      <ul className="grid grid-cols-[minmax(0,1fr)] max-w-[64rem] gap-x-12 gap-y-8 md:grid-cols-2">
        {items.map(([t, d]) => (
          <li key={t} className="border-t border-rule pt-4">
            <h3 className="text-[1.2rem] font-medium">{t}</h3>
            <p className="prose-body mt-1 text-ink-2">{d}</p>
          </li>
        ))}
      </ul>
    </Section>
  );
}

function Footer() {
  return (
    <footer className="border-t border-ink bg-paper-2">
      <div className="mx-auto max-w-[74rem] px-4 py-16 sm:px-8">
        <p className="max-w-[20ch] text-[2.1rem] leading-[1.08] font-medium tracking-[-0.015em] sm:text-[2.75rem]">
          Try it on your biggest repo.
        </p>
        <div className="mt-8 max-w-[44rem] space-y-3">
          <CopyCommand cmd={INSTALL} />
          <CopyCommand cmd="clove init && clove new my-branch" />
        </div>
        <div className="mt-14 flex flex-wrap justify-between gap-4 border-t border-rule pt-6 font-mono text-[0.75rem] text-muted">
          <span>clove · MIT or Apache-2.0</span>
          <a href={REPO} className="hover:text-rust">
            github.com/pierre-borckmans/clove
          </a>
        </div>
      </div>
    </footer>
  );
}

export function App() {
  return (
    <div id="top">
      <Nav />
      <Hero />
      <main>
        <Problem />
        <How />
        <Numbers />
        <Cli />
        <Agents />
        <Config />
        <Limits />
      </main>
      <Footer />
    </div>
  );
}
