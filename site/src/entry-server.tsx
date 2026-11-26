import { renderToStaticMarkup } from "react-dom/server";
import { App } from "./App";
import { Docs } from "./Docs";
import { pageFor, pages } from "./pages";

/** Every path the build writes, with its <title>. */
export const routes: { path: string; title: string }[] = [
  { path: "/", title: "clove: worktrees, ready in a second" },
  ...pages.map((p) => ({
    path: p.slug ? `/docs/${p.slug}/` : "/docs/",
    title: `${p.title} · clove docs`,
  })),
];

export function render(path: string): string {
  const page = pageFor(path);
  return renderToStaticMarkup(page ? <Docs page={page} /> : <App />);
}
