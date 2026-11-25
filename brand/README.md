# Brand

The mark is a lowercase c holding a clove: the c is the golden image, the rust clove the copy you work in.

- `make.py` generates every SVG from one geometry: `mark.svg`, `mark-on-dark.svg`, `icon.svg` (macOS app icon, 1024 grid), `favicon.svg`, `tray.svg` (menu-bar template).
- `render.mjs` rasterises them into `png/` with Playwright and prints a terminal bitmap; run it from `apps/desktop`: `node ../../brand/render.mjs`.
- The terminal mark used by the CLI and TUI is hand-tuned from that bitmap and lives in `crates/clove-core/src/brand.rs`.
- App icons: `pnpm tauri icon ../../brand/png/icon-1024.png -o src-tauri/icons` in `apps/desktop`, then copy `png/tray@2x.png` to `src-tauri/icons/tray.png`.

Colours: ink `#1a1714`, garlic ivory `#f6f1e7`, clove rust `#b4532a`, sage `#6b8f71`.
