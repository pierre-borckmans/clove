# Brand

The mark is a lowercase c holding a clove: the c is the golden image, the clove the copy you work in.

- `make.py` generates every SVG from one geometry: `mark.svg`, `mark-on-dark.svg`, `icon.svg` (macOS app icon, 1024 grid), `favicon.svg`, `tray.svg` (menu-bar template).
- `how-it-works.py` generates the README diagram, `how-it-works.svg` and `how-it-works-dark.svg`, from one layout: `python3 brand/how-it-works.py`.
- `render.mjs` rasterises them into `png/` with Playwright and prints a terminal bitmap; run it from `apps/desktop`: `node ../../brand/render.mjs`.
- The terminal mark used by the CLI and TUI is hand-tuned from that bitmap and lives in `crates/clove-core/src/brand.rs`.
- App icons: `pnpm tauri icon ../../brand/png/icon-1024.png -o src-tauri/icons` in `apps/desktop`, then copy `png/tray@2x.png` to `src-tauri/icons/tray.png`.

Colours (Bottle): ink `#16201b`, paper `#f1f2ec`, bottle green `#2f6b4c` (`#7cc49b` on dark), peeled clove `#e3c95e` with cream `#f3efd6` on the icon. Terminals use a mid green, `#3f8f63`, that reads on dark and light backgrounds.
