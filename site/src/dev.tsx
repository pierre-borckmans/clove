// Dev server only: the production build ships prerendered HTML and no React.
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { App } from "./App";

export function mountDev() {
  const el = document.getElementById("root");
  if (!el) return;
  const root = createRoot(el);
  flushSync(() => root.render(<App />));
}
