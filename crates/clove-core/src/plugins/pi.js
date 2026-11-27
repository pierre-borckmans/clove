// Managed by clove (`clove agents install`; `clove agents remove` removes this file).
// Pi's ExtensionAPI has no native WorktreeCreate/WorktreeRemove hook.
// Use repo-aware prompt guidance and guard the bash tool instead.
import { basename, dirname, isAbsolute, resolve } from "node:path";

const MANAGED_HINT = "This repository is managed by clove. Use `clove new <branch> --json` instead of `git worktree add`, and run `clove guide` for commands.";
const BLOCK_REASON = "This repository is managed by clove. Use `clove new <branch> --json` instead of `git worktree add` (run `clove guide` for details).";

// Keep this scanner in sync with opencode.js; both integrations are standalone.
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

export default function (pi) {
  const inFlight = new Map();

  async function isManaged(cwd) {
    if (!inFlight.has(cwd)) {
      const probe = (async () => {
        try {
          const res = await pi.exec("clove", ["hook", "check"], { cwd, timeout: 3000 });
          return res.code === 0 && JSON.parse(res.stdout)?.managed === true;
        } catch {
          return false;
        }
      })().finally(() => inFlight.delete(cwd));
      inFlight.set(cwd, probe);
    }
    return inFlight.get(cwd);
  }

  pi.on("before_agent_start", async (event, ctx) => {
    if (!await isManaged(ctx.cwd)) return;
    const guidelines = event.systemPromptOptions?.promptGuidelines;
    if (Array.isArray(guidelines)) {
      if (!guidelines.includes(MANAGED_HINT)) guidelines.push(MANAGED_HINT);
    } else if (typeof event.systemPrompt === "string" && !event.systemPrompt.includes(MANAGED_HINT)) {
      return { systemPrompt: event.systemPrompt + "\n\n" + MANAGED_HINT };
    }
  });

  pi.on("tool_call", async (event, ctx) => {
    if (event.toolName === "bash" && typeof event.input?.command === "string"
      && await targetsManagedRepo(event.input.command, ctx.cwd, isManaged)) {
      return { block: true, reason: BLOCK_REASON };
    }
  });
}
