import type { ReactNode } from "react";
import { Desktop, TuiScreen } from "./Apps";
import {
  accent,
  CopyCommand,
  dim,
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
    ["#app", "App"],
    ["#agents", "Agents"],
    ["#limits", "Limits"],
  ];
  return (
    <nav className="mx-auto flex max-w-[74rem] items-center justify-between gap-6 px-4 py-5 sm:px-8">
      <a href="#top" className="flex items-center gap-2 text-[1.35rem] font-semibold no-underline">
        <svg viewBox="0 0 100 100" className="size-6" aria-hidden="true">
          <path
            d="M73.04 29.26 A31 31 0 1 0 73.04 70.74"
            fill="none"
            stroke="currentColor"
            strokeWidth="19"
            strokeLinecap="round"
          />
          <path
            d="M50 8 C62 28 74 52 70 76 C68 90 32 90 30 76 C26 52 38 28 50 8Z"
            transform="translate(53 51) rotate(16) scale(0.33) translate(-50 -49)"
            fill="var(--rust)"
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
    "Vite and tsgo file watchers pick up edits inside a clone.",
    "Docker bind mounts from a clone work in OrbStack.",
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
            the slow part: 1m18s on the monorepo, once.
          </p>
        </Step>
        <Step n={2} title="clove new clones and mounts it">
          <p>
            <code>clonefile()</code> on the image rounds to 0.00s and shares every block with the
            golden image. Mounting it takes about 0.7s, most of the total.
          </p>
        </Step>
        <Step n={3} title="The mount becomes a real git worktree">
          <p>
            clove registers it with your repo and copies the golden index, so git doesn't rehash 35k
            files. Then it checks out your branch, about 0.2s.
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
              copy only when that clone writes to it. On the monorepo the golden image is 3.8G and a
              new clone owns 6.1M, the bytes it doesn't share. <code>clove ls</code> shows that
              number per clone.
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

function InitTerm() {
  const steps: [string, string][] = [
    ["create image", "0.62s"],
    ["mount", "0.44s"],
    ["fetch", "1.6s"],
    ["checkout", "5.8s"],
    ["warm mise", "5.7s"],
    ["warm direnv", "12.0s"],
    ["warm pnpm", "50.9s"],
    ["index", "0.27s"],
    ["unmount", "1.3s"],
  ];
  return (
    <Term title="~/src/mono">
      <Prompt />
      {"clove init\n"}
      <b className="font-semibold">clove init</b> mono <span className={dim}>~/src/mono</span>
      {"\n  base   origin/main\n  steps  mise, direnv-allow, direnv, pnpm"}
      <span className={dim}> (detected)</span>
      {"\n  env    auto\n\n"}
      {steps.map(([step, t]) => (
        <span key={step}>
          {"  "}
          <span className={ok}>✓</span> {step.padEnd(24)} <span className={dim}>{t}</span>
          {"\n"}
        </span>
      ))}
      {"\n"}
      <span className={ok}>●</span> golden image ready in 1m18s. Next:{" "}
      <b className="font-semibold">clove new &lt;branch&gt;</b>
    </Term>
  );
}

function Numbers() {
  const rows: [string, string, string][] = [
    ["clone image", "0.00s", "clonefile() of one file"],
    ["mount", "0.67-0.75s", "attach the APFS image"],
    ["fetch", "skipped", "fetched in the last 5 min"],
    ["checkout", "0.18-0.25s", "switch to your branch"],
    ["setup mise", "0.02s", "mise trust"],
    ["setup direnv-allow", "0.01-0.02s", "direnv allow"],
    ["setup direnv", "skipped", "flake unchanged"],
    ["setup pnpm", "skipped", "lockfile unchanged"],
  ];
  return (
    <Section id="numbers" label="the numbers" title="72 seconds down to one.">
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
          label={
            <>
              <code className="font-mono text-[0.85rem]">clove new</code>, lockfile unchanged
            </>
          }
          total="1.0s"
          emphasis
          segments={[{ s: 1.0, cls: "bg-rust" }]}
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

      <div className="mt-16 grid grid-cols-[minmax(0,1fr)] gap-10 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <div>
          <h3 className="text-[1.4rem] font-medium">Where the second goes</h3>
          <table className="mt-4 w-full border-t border-ink font-mono text-[0.78rem]">
            <thead>
              <tr className="border-b border-rule text-left text-[0.7rem] text-muted">
                <th className="py-2 font-normal">step</th>
                <th className="py-2 text-right font-normal">5 runs</th>
                <th className="hidden py-2 pl-6 font-normal sm:table-cell">what happens</th>
              </tr>
            </thead>
            <tbody>
              {rows.map(([step, t, what]) => (
                <tr key={step} className="border-b border-rule">
                  <td className="py-2 pr-4 whitespace-nowrap">{step}</td>
                  <td
                    className={`py-2 text-right whitespace-nowrap tabular-nums ${t === "skipped" ? "text-muted" : ""}`}
                  >
                    {t}
                  </td>
                  <td className="hidden py-2 pl-6 text-ink-2 sm:table-cell">{what}</td>
                </tr>
              ))}
              <tr className="border-b border-rule">
                <td className="py-2 font-semibold">total</td>
                <td className="py-2 text-right font-semibold whitespace-nowrap text-rust">
                  0.93-1.04s
                </td>
                <td className="hidden py-2 pl-6 text-ink-2 sm:table-cell">as clove reports it</td>
              </tr>
              <tr className="border-b border-ink">
                <td className="py-2">wall time</td>
                <td className="py-2 text-right whitespace-nowrap">1.04-1.16s</td>
                <td className="hidden py-2 pl-6 text-ink-2 sm:table-cell">process start to exit</td>
              </tr>
            </tbody>
          </table>
        </div>
        <Prose className="xl:pt-12">
          <p>
            Five consecutive <code>clove new</code> runs on the monorepo, release build. The mount
            is most of the time. The clone itself rounds to zero.
          </p>
          <p>
            clove skips the base fetch when the repo was fetched in the last five minutes. When it
            does fetch, that takes about 2s and runs in parallel with the mount.
          </p>
          <p>
            When the lockfile changed, the pnpm step runs with the golden image's{" "}
            <code>node_modules</code> already in place, so it installs only the difference. My first
            prototype, in bash, ran that check every time and took about 6s.
          </p>
        </Prose>
      </div>

      <div className="mt-16 grid grid-cols-[minmax(0,1fr)] gap-10 xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
        <div>
          <h3 className="text-[1.4rem] font-medium">The slow part, once</h3>
          <Prose className="mt-3">
            <p>
              <code>clove init</code> pays the 72 seconds, and a bit more, one time per repo. On the
              monorepo it took 1m18s, 51s of it in <code>pnpm install</code>.
            </p>
            <p>
              Warm steps sometimes leave daemons running on the golden volume. On this repo that was
              a bazel server and git's fsmonitor. Init stops them so the image can unmount, and
              tells you which ones it stopped.
            </p>
          </Prose>
        </div>
        <div className="min-w-0">
          <InitTerm />
        </div>
      </div>
    </Section>
  );
}

function Cli() {
  const cmds: [string, string][] = [
    ["clove init", "Build the golden image for this repo. Slow, once."],
    ["clove new <branch>", "Clone, mount, check out. Alias: add."],
    ["clove ls", "List clones: branch, changes, age, and the disk each one owns."],
    ["clove cd <clone>", "Jump into a clone. No argument: the main checkout."],
    ["clove run <clone> -- cmd", "Run a command in the clone's dev environment."],
    [
      "clove rm <clone>",
      "Unmount, delete the image and the git registration. Works from inside the clone.",
    ],
    ["clove refresh", "Build the next golden image from the latest base."],
    ["clove status", "Golden generation, age, commits behind, clones."],
    ["clove doctor", "Check the machine and the repo setup."],
    ["clove gc", "Find leftovers from interrupted runs."],
    ["clove mount --all-repos", "Remount everything after a reboot."],
    ["clove new --fetch", "Fetch the base even if it was fetched in the last 5 minutes."],
    ["clove new pr:123", "Check out pull request 123 as the local branch pr/123."],
    [
      "clove ui",
      "Terminal UI with mouse support and live progress. Quitting cds into the selected clone.",
    ],
    ["clove hook claude", "Print the Claude Code hooks that turn its worktrees into clones."],
    [
      "clove service install",
      "launchd agents: remount at login, refresh --all-repos nightly at 04:30.",
    ],
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
      <div className="mt-14">
        <h3 className="text-[1.4rem] font-medium">
          <code className="font-mono text-[1.15rem]">clove ui</code>
        </h3>
        <Prose className="mt-3 mb-6 text-[1rem]">
          <p>
            Everything above, in one screen: clones with their branch and changes, the golden image,
            and per-step timings for the selected clone. <code>clove new</code> and{" "}
            <code>clove refresh</code> show their steps live, and it works with the mouse.
          </p>
        </Prose>
        <TuiScreen />
      </div>
      <div className="mt-14 grid grid-cols-[minmax(0,1fr)] gap-8">
        <div className="prose-body max-w-[34rem] text-[1rem] text-ink-2">
          <h3 className="text-[1.4rem] font-medium text-ink">Shell integration</h3>
          <p className="mt-3">
            It lets <code>clove new</code> and <code>clove cd</code> change your directory, lets{" "}
            <code>clove rm</code> step out of the clone it removes, and adds completions for
            branches and clones.
          </p>
          <CopyCommand cmd={'eval "$(clove shell-init zsh)"'} className="mt-4" />
          <p className="mt-2 text-[0.9rem] text-muted!">
            bash works the same way. For fish, pipe it to <code>source</code>.
          </p>
        </div>
        <div className="min-w-0">
          <Term title="~/src/mono">
            <Prompt />
            {"clove ls\n"}
            <span className={dim}>
              {"   "}
              {["NAME", "BRANCH", "CHANGES", "AGE", "OWN"].map((h, i) =>
                h.padEnd([10, 10, 7, 3, 4][i] + 2),
              )}
              PATH
            </span>
            {"\n"}
            <span className={ok}>●</span>
            {"  "}
            <b className="font-semibold">{"feat-login".padEnd(12)}</b>
            <span className={accent}>{"feat/login".padEnd(12)}</span>
            <span className={dim}>{"clean".padEnd(9)}</span>
            {"1m".padEnd(5)}
            {"6.1M".padEnd(6)}
            <span className={dim}>~/src/mono.clove/feat-login/mono</span>
          </Term>
        </div>
      </div>
    </Section>
  );
}

function Agents() {
  return (
    <Section id="agents" label="agents" title="One clone per agent.">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-12">
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
            For Claude Code's own worktrees, <code>clove hook claude</code> prints a{" "}
            <code>settings.json</code> block with <code>WorktreeCreate</code> and{" "}
            <code>WorktreeRemove</code> hooks. With it, <code>claude --worktree</code> and the
            worktrees Claude Code makes for subagents become clones.
          </p>
          <p>
            Every command takes <code>--json</code> and prints machine-readable output on stdout,
            with progress on stderr. clove holds its repo lock only while copying the image, so
            several <code>clove new</code> calls can run at once.
          </p>
        </Prose>
        <div className="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-6 xl:grid-cols-2">
          <Term title="~/src/mono">
            <Prompt />
            {"clove new agent/parser -x claude\n"}
            <Prompt />
            {"clove new agent/cache  -x claude\n"}
            <Prompt />
            {"clove run agent-parser -- pnpm test"}
          </Term>
          <Term title="~/src/mono">
            <Prompt />
            {"clove new agent/parser --json | jq '{path, branch}'\n"}
            <Json
              value={{
                path: "/Users/you/src/mono.clove/agent-parser/mono",
                branch: "agent/parser",
              }}
            />
          </Term>
          <Term title="~/src/mono" className="xl:col-span-2">
            <Prompt />
            {"clove hook claude\n"}
            <Json text={CLAUDE_HOOKS} />
          </Term>
        </div>
      </div>
    </Section>
  );
}

const TOML = `base = "origin/main"
fetch_interval = 300

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

// Verbatim output of `clove hook claude` (CLAUDE_HOOKS in clove-cli).
const CLAUDE_HOOKS = `{
  "hooks": {
    "WorktreeCreate": [
      { "hooks": [{ "type": "command", "command": "clove hook worktree-create" }] }
    ],
    "WorktreeRemove": [
      { "hooks": [{ "type": "command", "command": "clove hook worktree-remove" }] }
    ]
  }
}`;

const PERSONAL = `# ~/.config/clove/repos/mono.toml
[[add_step]]
name = "build-deps"
warm = "pnpm run build-deps"
setup = "pnpm run build-deps"
watch = ["common/javascript"]`;

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
            <p>
              <code>fetch_interval</code> is in seconds: <code>clove new</code> skips fetching the
              base if the repo was fetched more recently than that. The default is 300.
            </p>
          </Prose>
        </div>
      </div>

      <div className="mt-16 grid grid-cols-[minmax(0,1fr)] gap-12 xl:grid-cols-[minmax(0,6fr)_minmax(0,5fr)]">
        <div className="min-w-0 xl:order-2">
          <Term title="~/.config/clove/repos/mono.toml">
            <Toml src={PERSONAL} />
          </Term>
        </div>
        <div className="xl:order-1">
          <h3 className="text-[1.4rem] font-medium">Your own steps on top</h3>
          <Prose className="mt-3 text-[1rem]">
            <p>
              Config comes in three layers, later ones winning:{" "}
              <code>~/.config/clove/config.toml</code>, then the repo's <code>.clove.toml</code>,
              then <code>~/.config/clove/repos/&lt;repo&gt;.toml</code> for what only you need.{" "}
              <code>[[add_step]]</code> appends to the steps instead of replacing them.
            </p>
            <p>
              On the monorepo I added a <code>build-deps</code> step that builds the shared
              JavaScript packages. <code>clove refresh</code> took 22.5s with it, a new clone still
              took 1.02s, and the frontend typecheck inside that clone passed with 0 errors in 18s.
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
        <code>clove cd</code> remounts the one you ask for, and <code>clove service install</code>{" "}
        remounts everything at login.
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
        <code>clove doctor</code> warns past 200 commits behind. The launchd service refreshes every
        repo nightly.
      </>,
    ],
    [
      "The first git status can be slow.",
      <>
        The copied index means git doesn't rehash the tree, but the first <code>git status</code> in
        a clone on the monorepo took 1.5s because it started git's fsmonitor daemon.
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
        <Desktop />
        <Agents />
        <Config />
        <Limits />
      </main>
      <Footer />
    </div>
  );
}
