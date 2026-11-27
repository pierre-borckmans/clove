// Managed by clove (`clove agents install`; `clove agents remove` removes this file).
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { basename, dirname, isAbsolute, resolve } from "node:path";

const MANAGED_HINT = "This repository is managed by clove. Use `clove new <branch> --json` instead of `git worktree add`, and run `clove guide` for commands.";
const BLOCK_REASON = "This repository is managed by clove. Use `clove new <branch> --json` instead of `git worktree add` (run `clove guide` for details).";

// Keep this scanner in sync with pi.js; both integrations are standalone.
// This is a guardrail for literal shell commands, not a shell interpreter.
function gitWorktreeAdds(command, baseCwd) {
  const takesValue = /^(?:-C|-c|--git-dir|--work-tree|--namespace|--super-prefix|--config-env)$/;
  const globalFlag = /^(?:-p|-P|--paginate|--no-pager|--bare|--no-replace-objects|--no-lazy-fetch|--no-optional-locks|--no-advice|--(?:literal|glob|noglob|icase)-pathspecs|--(?:git-dir|work-tree|namespace|super-prefix|config-env|exec-path)=.*)$/s;
  const control = /^(?:if|then|elif|else|do|while|until|time|!|\{)$/;
  const wrapper = /^(?:exec|command|builtin|nohup|sudo)$/;
  const declaration = /^(?:declare|local|typeset|export|readonly)$/;
  const invocations = [];
  let i = 0;

  function directory(cwd, arg) {
    if (arg.includes("\0")) return null;
    if (isAbsolute(arg)) return resolve(arg);
    return cwd === null ? null : resolve(cwd, arg);
  }

  function gitTarget(cwd, explicit) {
    if (explicit === undefined) return cwd;
    if (!explicit) return null;
    const dir = directory(cwd, explicit);
    return dir !== null && basename(dir) === ".git" ? dirname(dir) : dir;
  }

  function word(end, cwd, exportedGitDir, literal = false) {
    const start = i;
    let value = "";
    let quote = "";
    while (i < command.length) {
      const ch = command[i];
      if (!quote && (ch === end || /[ \t\r\n;&|()<>]/.test(ch))) break;
      i++;
      if (ch === quote) {
        quote = "";
      } else if (!quote && (ch === "'" || ch === '"')) {
        quote = ch;
      } else if (quote !== "'" && ch === "\\") {
        const next = command[i];
        if (next === undefined) break;
        if (!quote || /[\\$`"\n]/.test(next)) {
          i++;
          if (next !== "\n") value += next;
        } else {
          value += ch;
        }
      } else if (!literal && quote !== "'" && ch === "$" && command[i] === "(") {
        i++;
        if (command[i] === "(") {
          // Arithmetic expansions are not shell command positions.
          while (i < command.length && !command.startsWith("))", i)) i++;
          i += 2;
        } else {
          scan(")", cwd, exportedGitDir);
        }
        value += "\0"; // an expansion is not a literal shell word
      } else if (!literal && quote !== "'" && ch === "`") {
        scan("`", cwd, exportedGitDir);
        value += "\0";
      } else if (!literal && quote !== "'" && ch === "$" && command[i] === "{") {
        // Braced parameter expansions can contain spaces and nested expansions.
        i++;
        let depth = 1;
        while (i < command.length && depth) {
          const part = command[i++];
          if (part === "\\") i++;
          else if (part === "{") depth++;
          else if (part === "}") depth--;
        }
        value += "\0";
      } else if (!literal && ((quote !== "'" && ch === "$" && /[\w@*#?$!-]/.test(command[i] ?? ""))
        || (!quote && (/[*?[\]]/.test(ch) || (i === start + 1 && ch === "~"))))) {
        value += "\0";
      } else {
        value += ch;
      }
    }
    return value;
  }

  function skipArray(cwd, exportedGitDir) {
    let depth = 1;
    while (i < command.length && depth) {
      const start = i;
      if (command[i] === "(") depth++;
      else if (command[i] === ")") depth--;
      else if (command[i] === "#") {
        while (i < command.length && command[i] !== "\n") i++;
      } else word(")", cwd, exportedGitDir);
      if (i === start) i++;
    }
  }

  function skipHeredocs(heredocs) {
    for (const { delimiter, stripTabs } of heredocs) {
      while (i < command.length) {
        const next = command.indexOf("\n", i);
        const line = command.slice(i, next === -1 ? undefined : next);
        i = next === -1 ? command.length : next + 1;
        if ((stripTabs ? line.replace(/^\t+/, "") : line) === delimiter) break;
      }
    }
    heredocs.length = 0;
  }

  function scan(end, initialDir, initialGitDir) {
    let cwd = initialDir;
    let exportedGitDir = initialGitDir;
    const heredocs = [];
    let stage = "command";
    let gitCwd, gitDir, envGitDir, gitArg, cdArg, pendingCd;
    let continuing = false;
    let pipeline = false;
    let env = false;
    let envArg = false;
    let ignoreExport = false;
    let wrapperSeen = false;
    let declareExports = false;
    let gitDirChange;
    while (i < command.length) {
      const ch = command[i];
      if (ch === end) {
        i++;
        return;
      }
      if (command.startsWith("\\\n", i)) {
        i += 2;
      } else if (/[ \t\r]/.test(ch)) {
        i++;
      } else if (ch === "#") {
        while (i < command.length && command[i] !== "\n") i++;
      } else if (/[;\n&|()]/.test(ch) && !command.startsWith("&>", i)) {
        const op = command.slice(i, i + 2);
        const and = op === "&&";
        const or = op === "||";
        const pipe = ch === "|" && !or;
        const sequence = ch === ";" || ch === "\n";
        if (ch === "\n" && continuing) {
          i++;
          skipHeredocs(heredocs);
          continue;
        }
        // The RHS of `cd ... ||` sees the original cwd on failure. After
        // the list, only a terminating fallback leaves a definite cwd.
        if (!pipeline && !pipe && (sequence || and || or)) {
          const assignmentOnly = stage === "command" && !env && !wrapperSeen && envGitDir !== undefined;
          const change = gitDirChange ?? (assignmentOnly && exportedGitDir !== undefined
            ? { value: envGitDir.includes("\0") ? null : envGitDir }
            : undefined);
          if (change) {
            const { value } = change;
            exportedGitDir = pendingCd && value !== exportedGitDir ? null : value;
          }
        }
        if (pendingCd && (sequence || and || or)) {
          cwd = stage === "leave" && !pipeline ? pendingCd.target : null;
          pendingCd = undefined;
        } else if (!pipeline && !pipe && stage === "cd") {
          const target = cdArg && cdArg !== "-" ? directory(cwd, cdArg) : null;
          if (or) pendingCd = { target };
          else if (sequence || and) cwd = target;
        }
        i += and || or || op === "|&" ? 2 : 1;
        continuing = and || or || pipe;
        if (ch !== "(") pipeline = pipe;
        stage = "command";
        env = envArg = ignoreExport = wrapperSeen = declareExports = false;
        envGitDir = gitDirChange = undefined;
        if (ch === "(") scan(")", cwd, exportedGitDir);
        if (ch === "\n") skipHeredocs(heredocs);
      } else if (/[<>&]/.test(ch)) {
        const hereString = command.startsWith("<<<", i);
        const heredoc = !hereString && command.startsWith("<<", i);
        const stripTabs = heredoc && command.startsWith("<<-", i);
        if (hereString || stripTabs) i += 3;
        else if (heredoc) i += 2;
        else {
          i++;
          if (/[<>&|]/.test(command[i] ?? "")) i++;
        }
        while (/[ \t]/.test(command[i] ?? "")) i++;
        const target = word(end, cwd, exportedGitDir, heredoc);
        if (heredoc) heredocs.push({ delimiter: target, stripTabs });
      } else {
        continuing = false;
        const array = (stage === "command" || stage === "declare") && /^[A-Za-z_]\w*\+?=\(/.exec(command.slice(i));
        if (array) {
          i += array[0].length;
          skipArray(cwd, exportedGitDir);
          continue;
        }
        const start = i;
        const arg = word(end, cwd, exportedGitDir);
        if (stage === "command") {
          const raw = command.slice(start, i);
          if (envArg) {
            if (arg === "GIT_DIR") {
              envGitDir = undefined;
              ignoreExport = true;
            }
            envArg = false;
            continue;
          }
          if (arg === "env") {
            env = true;
            continue;
          }
          if (env && (arg === "-u" || arg === "--unset")) {
            envArg = true;
            continue;
          }
          if (env && /^(?:-i|--ignore-environment|-u.+|--unset=.+|--|-)$/s.test(arg)) {
            if (/^(?:-i|--ignore-environment|-|(?:-u|--unset=)GIT_DIR)$/.test(arg)) {
              envGitDir = undefined;
              ignoreExport = true;
            }
            continue;
          }
          if (raw.startsWith("GIT_DIR=") || (env && arg.startsWith("GIT_DIR="))) {
            envGitDir = arg.slice("GIT_DIR=".length);
            continue;
          }
          if (/^[A-Za-z_]\w*\+?=/.test(raw) || control.test(raw)) continue;
          if (wrapper.test(arg)) {
            wrapperSeen = true;
            continue;
          }
          if (declaration.test(arg)) {
            stage = "declare";
            declareExports = arg === "export";
          } else if (arg === "unset") stage = "unset";
          else if (arg === "exit" || arg === "return") stage = "leave";
          else stage = arg === "git" ? "git" : arg === "cd" ? "cd" : null;
          gitCwd = cwd;
          gitDir = gitArg = cdArg = undefined;
        } else if (stage === "declare") {
          if (/^-[A-Za-z]*x[A-Za-z]*$/.test(arg)) declareExports = true;
          if (declareExports && arg.startsWith("GIT_DIR=")) {
            const value = arg.slice("GIT_DIR=".length);
            gitDirChange = { value: value.includes("\0") ? null : value };
          }
        } else if (stage === "unset") {
          if (arg === "GIT_DIR") gitDirChange = { value: undefined };
        } else if (stage === "cd") {
          if (cdArg === undefined && /^(?:--|-[LPe@]+)$/.test(arg)) continue;
          cdArg = cdArg === undefined ? arg : "\0";
        } else if (stage === "git") {
          if (gitArg) {
            if (gitArg === "-C" && arg !== "") gitCwd = directory(gitCwd, arg);
            if (gitArg === "--git-dir") gitDir = arg;
            gitArg = undefined;
          } else if (takesValue.test(arg)) {
            gitArg = arg;
          } else if (arg.startsWith("--git-dir=")) {
            gitDir = arg.slice("--git-dir=".length);
          } else if (!globalFlag.test(arg)) {
            stage = arg === "worktree" ? "worktree" : null;
          }
        } else if (stage === "worktree") {
          if (arg === "add") {
            const explicit = gitDir ?? envGitDir ?? (ignoreExport ? undefined : exportedGitDir);
            invocations.push({ targetDir: gitTarget(gitCwd, explicit) });
          }
          stage = null;
        }
      }
    }
  }

  scan(undefined, baseCwd);
  return invocations;
}

async function targetsManagedRepo(command, cwd, isManaged) {
  const dirs = new Set(gitWorktreeAdds(command, cwd).map(({ targetDir }) => targetDir));
  for (const dir of dirs) {
    if (dir !== null && await isManaged(dir)) return true;
  }
  return false;
}

function abortReason(signal) {
  return signal.reason ?? new Error("clove canceled");
}

function runClove(args, { cwd, stdin, signal, timeoutMs, onCanceled } = {}) {
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
        onCanceled?.(stderr);
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

async function guard(tool, input, cwd) {
  if ((tool === "bash" || tool === "shell") && typeof input?.command === "string"
    && await targetsManagedRepo(input.command, resolve(cwd, input.workdir || input.cwd || "."), isManaged)) {
    throw new Error(BLOCK_REASON);
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
            if (signal?.aborted) throw abortReason(signal);
            const requestId = randomUUID();
            let stderr;
            try {
              const stdout = await runClove(["hook", "opencode-worktree-create"], {
                cwd: sourceDirectory,
                stdin: JSON.stringify({ sourceDirectory, directory, branch, requestId }),
                signal,
                onCanceled: (errors) => { stderr = errors; },
              });
              if (signal?.aborted) throw abortReason(signal);
              return { directory: JSON.parse(stdout).directory };
            } catch (error) {
              if (!signal?.aborted) throw error;
              const name = /^clove:creating:([A-Za-z0-9_][A-Za-z0-9_.-]*)\r?$/m.exec(stderr ?? error?.stderr ?? "")?.[1];
              await runClove(["hook", "opencode-worktree-abort"], {
                stdin: JSON.stringify({ projectDirectory: projectDir, requestId, name }),
              }).catch((rollbackError) => {
                console.error(`clove: rollback failed for worktree request ${requestId}`, rollbackError);
              });
              throw abortReason(signal);
            }
          },
          async remove(input, context) {
            const directory = input.directory;
            const force = Boolean(input.force);
            try {
              await runClove(["hook", "opencode-worktree-remove"], {
                stdin: JSON.stringify({ directory, force, projectDirectory: projectDir }),
                signal: context?.signal,
              });
            } catch (error) {
              if (context?.signal?.aborted || force || !(error?.code === 2 || /has uncommitted changes|contains modified or untracked files/i.test(error?.stderr ?? ""))) {
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
    if (ctx.tool?.hook) {
      await ctx.tool.hook("execute.before", (event) =>
        guard(event.tool, event.input, ctx.location?.directory || process.cwd()));
    }
    if (managed && ctx.session?.hook) {
      await ctx.session.hook("context", (event) => {
        if (Array.isArray(event.system)) event.system.push({ type: "text", text: MANAGED_HINT });
      });
    }
  },
  // V1 supports context and tool hooks, but not worktree strategies.
  async server(input) {
    const dir = input?.directory || input?.worktree || process.cwd();
    const managed = await isManaged(dir);
    return {
      "tool.execute.before": (toolInput, output) => guard(toolInput?.tool, output?.args, dir),
      "experimental.chat.system.transform": async (_sysInput, output) => {
        if (managed && Array.isArray(output?.system)) output.system.push(MANAGED_HINT);
      },
    };
  },
};
