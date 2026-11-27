// Managed by clove (`clove agents install`; `clove agents remove` removes this file).
const MANAGED_HINT = "This repository is managed by clove. Use `clove new <branch> --json` instead of `git worktree add`, and run `clove guide` for commands.";

export default function (pi) {
  pi.on("before_agent_start", async (event, ctx) => {
    let managed;
    try {
      const res = await pi.exec("clove", ["hook", "check"], { cwd: ctx.cwd, timeout: 3000 });
      managed = res.code === 0 && JSON.parse(res.stdout)?.managed === true;
    } catch {
      return;
    }
    if (!managed) return;
    const opts = event.systemPromptOptions;
    if (opts && opts.forceSystemPrompt === undefined && !opts.customPrompt && Array.isArray(opts.promptGuidelines)) {
      if (!opts.promptGuidelines.includes(MANAGED_HINT)) opts.promptGuidelines.push(MANAGED_HINT);
    } else if (opts && opts.forceSystemPrompt === undefined && opts.customPrompt) {
      const existing = opts.appendSystemPrompt ?? "";
      if (!existing.includes(MANAGED_HINT)) {
        opts.appendSystemPrompt = existing ? `${existing}\n\n${MANAGED_HINT}` : MANAGED_HINT;
      }
    } else if (typeof event.systemPrompt === "string" && !event.systemPrompt.includes(MANAGED_HINT)) {
      return { systemPrompt: `${event.systemPrompt}\n\n${MANAGED_HINT}` };
    }
  });
}
