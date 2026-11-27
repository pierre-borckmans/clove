# Coding agents

[← README](../README.md)

`clove agents install` teaches Claude Code, Codex, OpenCode and Pi to use clove. The instructions go in a short block between markers, leaving the rest of each file alone. Agents read `clove guide`, which matches the installed version.

## Claude Code

Installs a skill at `~/.claude/skills/clove/SKILL.md`, adds the instructions to `~/.claude/CLAUDE.md`, and adds native `WorktreeCreate` and `WorktreeRemove` hooks to `~/.claude/settings.json`. In repos clove manages, `claude --worktree` and subagent worktrees become clones. Elsewhere, the hooks make the same plain worktree Claude would.

## Codex

Adds the instructions to `~/.codex/AGENTS.md`.

## OpenCode

Installs `skills/clove/SKILL.md` and `plugins/clove.js` in OpenCode's config directory: `~/.config/opencode` by default, `$XDG_CONFIG_HOME/opencode` with an XDG config home, or `$OPENCODE_CONFIG_DIR` when set.

Adds the instructions to `AGENTS.md` there. If that file is absent and `~/.claude` is a directory (or `~/.claude/CLAUDE.md` exists), clove leaves `AGENTS.md` absent so OpenCode reads `~/.claude/CLAUDE.md`, which clove creates or updates.

- **V2:** Registers a native `clove` worktree strategy to create, remove and list worktrees in repos clove manages. Other repos keep OpenCode's default Git strategy. Both strategies keep the branch when removing a worktree. The plugin also blocks `git worktree add` in the `bash` tool and adds a hint to session context in managed repos.
- **V1 (1.18.29+):** Has no native worktree strategy hook. The same plugin uses `tool.execute.before` to block `git worktree add` and adds a hint to system context in managed repos.

The plugin checks whether a repo is managed when loaded. Reload or restart OpenCode after `clove init` or `clove deinit`.

Before uninstalling the plugin, remove any OpenCode worktrees that use the `clove` strategy, either through OpenCode or with `clove rm`. OpenCode tracks which strategy owns each worktree.

## Pi

Installs `skills/clove/SKILL.md` and `extensions/clove.js` in `~/.pi/agent`, or `$PI_CODING_AGENT_DIR` if set. It adds the instructions to the first existing global context file in Pi's order: `AGENTS.override.md`, `AGENTS.md`, `AGENTS.MD`, `CLAUDE.md`, `CLAUDE.MD`. If none exists, it creates `AGENTS.md`.

Pi's `ExtensionAPI` has no native `WorktreeCreate` or `WorktreeRemove` hooks. The extension uses `before_agent_start` to add a prompt guideline in repos clove manages, and `tool_call` to block `git worktree add` and point the agent to `clove new`. It leaves other repos alone.

## Commands

- `clove hook claude` prints the settings block.
- `clove hook opencode` prints the self-contained plugin.
- `clove hook pi` prints the self-contained extension.
- `clove agents status` shows what's installed.
- `clove agents remove` takes the integration out.
- `clove doctor` flags missing or outdated integrations.
