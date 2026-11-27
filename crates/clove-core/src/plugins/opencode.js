// Managed by clove (`clove agents install`; `clove agents remove` removes this file).
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";

const MANAGED_HINT = "This repository is managed by clove. Use `clove new <branch> --json` instead of `git worktree add`, and run `clove guide` for commands.";

function abortReason(signal) {
  return signal.reason ?? new Error("clove canceled");
}

function runClove(args, { cwd, stdin, signal, timeoutMs } = {}) {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(abortReason(signal));
      return;
    }
    const detached = process.platform !== "win32";
    const child = spawn("clove", args, { cwd, detached, stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    let timer, killTimer, stopReason, spawnError;
    const killGroup = (sig) => {
      if (detached && child.pid) {
        try {
          process.kill(-child.pid, sig);
          return;
        } catch {
          // The group may already be gone.
        }
      }
      child.kill(sig);
    };
    const stop = (error) => {
      if (stopReason !== undefined) return;
      stopReason = error;
      killGroup("SIGTERM");
      killTimer = setTimeout(() => killGroup("SIGKILL"), 2000);
      killTimer.unref();
    };
    const onAbort = () => stop(abortReason(signal));
    child.stdout.setEncoding("utf8").on("data", (chunk) => { stdout += chunk; });
    child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
    // A child that exits before consuming JSON can close its stdin early.
    child.stdin.on("error", () => {});
    child.on("error", (error) => { spawnError ??= error; });
    child.once("close", (code, killedBy) => {
      clearTimeout(timer);
      clearTimeout(killTimer);
      signal?.removeEventListener("abort", onAbort);
      if (stopReason !== undefined) {
        // The leader was reaped, but descendants with no inherited pipes
        // may still be alive. Don't let those race the cancellation cleanup.
        if (detached) killGroup("SIGKILL");
        reject(stopReason);
      } else if (spawnError) reject(spawnError);
      else if (code === 0) resolve(stdout);
      else reject(Object.assign(new Error(stderr.trim() || `clove exited (${code ?? killedBy})`), { code, stderr }));
    });
    signal?.addEventListener("abort", onAbort, { once: true });
    if (timeoutMs != null) timer = setTimeout(() => stop(new Error("clove timed out")), timeoutMs);
    child.stdin.end(stdin);
  });
}

async function isManaged(cwd) {
  try {
    const stdout = await runClove(["hook", "check"], { cwd, timeoutMs: 3000 });
    return JSON.parse(stdout)?.managed === true;
  } catch {
    return false;
  }
}

export default {
  id: "clove",
  async setup(ctx) {
    const projectDir = ctx.location?.project?.canonical || ctx.location?.directory || process.cwd();
    const managed = await isManaged(projectDir);
    // Registering a strategy makes it the recorded owner of created worktrees.
    // Unmanaged repositories should keep OpenCode's default Git strategy.
    if (managed && ctx.worktree?.transform) {
      await ctx.worktree.transform((editor) => {
        editor.add({
          id: "clove",
          async create(input, context) {
            const { sourceDirectory, directory, branch } = input;
            const signal = context?.signal;
            const requestId = randomUUID();
            try {
              const stdout = await runClove(["hook", "opencode-worktree-create"], {
                cwd: sourceDirectory,
                stdin: JSON.stringify({ sourceDirectory, directory, branch, requestId }),
                signal,
              });
              if (signal?.aborted) throw abortReason(signal);
              return { directory: JSON.parse(stdout).directory };
            } catch (error) {
              if (!signal?.aborted) throw error;
              await runClove(["hook", "opencode-worktree-abort"], {
                stdin: JSON.stringify({ projectDirectory: projectDir, requestId }),
              }).catch((rollbackError) => {
                console.error(`clove: rollback failed for worktree request ${requestId}`, rollbackError);
              });
              throw abortReason(signal);
            }
          },
          async remove(input, context) {
            const directory = input.directory;
            const force = Boolean(input.force);
            if (context?.signal?.aborted) throw abortReason(context.signal);
            try {
              await runClove(["hook", "opencode-worktree-remove"], {
                stdin: JSON.stringify({ directory, force, projectDirectory: projectDir }),
              });
            } catch (error) {
              if (force || !(error?.code === 2 || /has uncommitted changes|contains modified or untracked files/i.test(error?.stderr ?? ""))) {
                throw error;
              }
              const message = error.message;
              let operationError;
              try {
                const { Worktree } = await import("@opencode/plugin");
                operationError = new Worktree.OperationError({ message, forceRequired: true });
              } catch {
                operationError = Object.assign(new Error(message), { _tag: "Worktree.OperationError", forceRequired: true });
              }
              throw operationError;
            }
          },
          async list(sourceDirectory, context) {
            const stdout = await runClove(["hook", "opencode-worktree-list"], {
              cwd: sourceDirectory,
              stdin: JSON.stringify({ sourceDirectory }),
              signal: context?.signal,
            });
            return JSON.parse(stdout);
          },
        });
      });
    }
    if (managed && ctx.session?.hook) {
      await ctx.session.hook("context", (event) => {
        if (Array.isArray(event.system)) event.system.push({ type: "text", text: MANAGED_HINT });
      });
    }
  },
  // V1 supports context hooks, but not worktree strategies.
  async server(input) {
    const dir = input?.directory || input?.worktree || process.cwd();
    const managed = await isManaged(dir);
    return {
      "experimental.chat.system.transform": async (_sysInput, output) => {
        if (managed && Array.isArray(output?.system)) output.system.push(MANAGED_HINT);
      },
    };
  },
};
