# Configuration

[← README](../README.md)

- **clove detects steps from the repo:** mise, direnv (including nix-direnv's cache), cargo, uv, pnpm, bun, yarn and npm. Anything else (Go, Gradle, codegen) is a step you add.
- `clove config` shows what it found; `clove config --write` saves it to `.clove.toml` so the team gets the same setup.

```toml
base = "origin/main"

[[step]]
name = "pnpm"
warm = "pnpm install --frozen-lockfile"                 # in the golden image, on init/refresh
setup = "pnpm install --prefer-offline --frozen-lockfile" # in each new clone...
watch = ["pnpm-lock.yaml", "pnpm-workspace.yaml"]       # ...only if these changed

[[step]]
name = "codegen"
warm = "make generate"
setup = "make generate"
watch = ["proto/"]
```

**Local-only files** like `.env` or `.envrc.local` never reach a fresh checkout. clove:

- detects the ones git ignores (`**/.env`, `**/.env.*`, `**/.envrc.local`, `**/.dev.vars`)
- copies them into the golden image before warming, so codegen can read them
- copies them again into each new clone, so it gets their current contents

List them yourself to override:

```toml
copy = [".env", "apps/*/.env.local", "config/dev.secrets.json"]

[refresh]
auto = true        # refresh in the background when `clove new` finds the image stale
max_age = "24h"     # safety net; steps' watch paths are the main signal
max_behind = 100   # commits

[prune]
auto = false       # the service removes merged, clean, idle clones; `clove log` lists them
```

- Steps run inside the repo's environment (`direnv exec` or `mise exec`).
- `[[step]]` replaces the detected steps; `[[add_step]]` appends to them.
- Settings layer in this order, last wins:
  1. `~/.config/clove/config.toml`: personal defaults for every repo
  2. `.clove.toml` in the repo: shared with the team
  3. `~/.config/clove/repos/<repo>.toml`: personal overrides for one repo

For example, a monorepo whose workspace libraries must be built before the app typechecks:

```toml
# ~/.config/clove/repos/mono.toml
[[add_step]]
name = "build-deps"
warm = "pnpm run build-deps"
setup = "pnpm run build-deps"
watch = ["common/javascript", "packages/inkwell", "pnpm-lock.yaml"]
```
