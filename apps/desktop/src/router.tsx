import { createHashHistory, createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { AppShell } from "@/components/app-shell";
import { CleanupPage } from "@/routes/cleanup";
import { DoctorPage } from "@/routes/doctor";
import { HomePage } from "@/routes/home";
import { PopoverPage } from "@/routes/popover";
import { RepoPage } from "@/routes/repo";
import { SettingsPage } from "@/routes/settings";

const rootRoute = createRootRoute({ component: Outlet });

const shellRoute = createRoute({ getParentRoute: () => rootRoute, id: "shell", component: AppShell });

const routeTree = rootRoute.addChildren([
  shellRoute.addChildren([
    createRoute({ getParentRoute: () => shellRoute, path: "/", component: HomePage }),
    createRoute({ getParentRoute: () => shellRoute, path: "/repo/$repoId", component: RepoPage }),
    createRoute({ getParentRoute: () => shellRoute, path: "/doctor", component: DoctorPage }),
    createRoute({ getParentRoute: () => shellRoute, path: "/cleanup", component: CleanupPage }),
    createRoute({
      getParentRoute: () => shellRoute,
      path: "/settings",
      component: SettingsPage,
      validateSearch: (s: Record<string, unknown>): { repo?: string } => ({
        repo: typeof s.repo === "string" ? s.repo : undefined,
      }),
    }),
  ]),
  createRoute({ getParentRoute: () => rootRoute, path: "/popover", component: PopoverPage }),
]);

// Hash history: the bundled app is one index.html, so deep links must not hit the asset server.
export const router = createRouter({ routeTree, history: createHashHistory(), defaultPreload: false });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
