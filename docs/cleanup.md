# Cleaning up merged clones

[← README](../README.md)

`clove prune` asks GitHub (through `gh`) for each clone's pull request.

- **A clone goes when** its PR is merged, its tip is the commit GitHub merged (or one the PR contains), and it has no uncommitted changes. That catches squash merges, which `git branch --merged` misses.
- **A clone is kept, and listed for review,** if it has commits after the merge or local edits. So is any branch without a PR.
- It lists first and asks; `--apply` skips the question.

With `prune.auto` on (off by default), the service prunes on its own:

- only clones proven merged and clean, that git has been quiet in for an hour
- never plain git worktrees
- nothing at all when GitHub can't be asked
- a clone something still has open is removed as soon as it's free

```toml
[prune]
auto = true
```

**Every removal is recorded,** whether by hand, by prune or by the service. `clove log` shows what went, when, by what and why, plus the `git branch` command that brings a deleted branch back. A command on a removed clone says what happened to it.
