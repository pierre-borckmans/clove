import "./styles.css";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import { setupTheme } from "@/hooks/use-theme";
import { isTauri } from "@/lib/bridge";
import { wireOps } from "@/lib/ops";
import { router } from "./router";

setupTheme();
document.documentElement.dataset.shell = isTauri ? "native" : "web";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: true, staleTime: 1500 },
  },
});

const isPopover = window.location.hash.startsWith("#/popover");
wireOps(queryClient, { toasts: true });
if (isPopover) document.documentElement.dataset.window = "popover";

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delayDuration={500} skipDelayDuration={200}>
          <RouterProvider router={router} />
        </TooltipProvider>
      </QueryClientProvider>
    </StrictMode>,
  );
}
