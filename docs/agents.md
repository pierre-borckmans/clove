# Coding agents

[← README](../README.md)

`clove agents install` teaches Claude Code and Codex to use clove:

- a Claude Code skill (`~/.claude/skills/clove`)
- a short block in `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md`, between markers, leaving the rest of each file as it was
- Claude Code's worktree hooks in `~/.claude/settings.json`, so `claude --worktree` and subagent worktrees become clones. In repos clove doesn't manage, the hooks make the same plain worktree Claude would.

Agents then read `clove guide`, written for them and matching the installed version.

- `clove agents status` shows what's installed
- `clove agents remove` takes it out
- `clove doctor` flags it when missing or outdated
