import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: { target: "es2022" },
  // The docs pages read the repo's README.md and docs/, one level up.
  server: { fs: { allow: [".."] } },
});
