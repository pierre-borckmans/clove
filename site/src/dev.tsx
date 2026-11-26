// Dev server only: the production build ships prerendered HTML and no React.
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Docs } from "./Docs";
import { pageFor } from "./pages";

export function mountDev() {
  const el = document.getElementById("root");
  if (!el) return;
  const page = pageFor(location.pathname);
  const root = createRoot(el);
  flushSync(() => root.render(page ? <Docs page={page} /> : <App />));
}
