import { flushSync } from "react-dom";

/**
 * Runs a state change inside a view transition, so the main pane crossfades
 * (styles.css). Where view transitions aren't supported it just runs.
 */
export function withTransition(update: () => void, type = "fade") {
  const start = document.startViewTransition?.bind(document);
  if (!start) return update();
  const run = () => flushSync(update);
  // Typed transitions are newer than plain ones; fall back to a plain crossfade.
  if (CSS.supports?.("selector(:active-view-transition-type(a))")) start({ update: run, types: [type] });
  else start(run);
}
