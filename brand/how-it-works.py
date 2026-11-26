#!/usr/bin/env python3
"""Generates the README's "How it works" diagram, light and dark, from one layout.

    python3 brand/how-it-works.py   # writes brand/how-it-works.svg and how-it-works-dark.svg
"""
from pathlib import Path

W, H = 990, 500
SERIF = "Iowan Old Style, Palatino, Georgia, serif"
MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
SANS = "-apple-system, BlinkMacSystemFont, Segoe UI, Helvetica, Arial, sans-serif"

THEMES = {
    "light": dict(
        card="#f1f2ec", card_line="#d5dbd3", ink="#16201b", muted="#6f7a73",
        golden="#2f6b4c", golden_ink="#f1f2ec", golden_sub="#bfe0cb", layer="#7cc49b", golden_line="none",
        arrow="#89948d", cow="#2f6b4c", clove="#e3c95e", chip="#e4eae3", chip_ink="#2f6b4c",
    ),
    "dark": dict(
        card="#18201b", card_line="#2f3a33", ink="#e4eae3", muted="#8f9a93",
        golden="#2f6b4c", golden_ink="#f1f2ec", golden_sub="#a9dcbf", layer="#2a6046", golden_line="#7cc49b",
        arrow="#5f6b64", cow="#7cc49b", clove="#e3c95e", chip="#22302a", chip_ink="#93d0ac",
    ),
}

# (title, mono line or None, chips or None, note or None)
SOURCES = [
    ("base branch", "origin/main", None, None),
    ("local files", ".env  .envrc.local", None, None),
    ("detected steps", None, ["mise", "direnv + nix", "cargo", "uv", "pnpm", "bun", "yarn", "npm"],
     "+ your own, in .clove.toml"),
]
GOLDEN_ROWS = ["dependencies installed", "dev shells evaluated", "builds + caches warm", "git index, mtimes kept"]
CLONES = [
    ("feat/login", "~1s"),
    ("pr/123", "~1s"),
    ("fix/flaky-test", "~1s"),
]

SRC_X, CARD_W, CARD_H = 24, 220, 64
GAP = 16
GX, GY, GW, GH = 334, 88, 280, 318
CL_X = 746
CHIP_H, CHIP_GAP = 20, 6


def chip_w(label):
    return 7.2 * len(label) + 16


def chip_rows(chips, width):
    rows, row, used = [], [], 0
    for c in chips:
        w = chip_w(c)
        if row and used + CHIP_GAP + w > width:
            rows.append(row)
            row, used = [], 0
        used += (CHIP_GAP if row else 0) + w
        row.append(c)
    if row:
        rows.append(row)
    return rows


def source_height(src):
    _, line, chips, note = src
    h = 40
    if line:
        h += 16
    if chips:
        h += len(chip_rows(chips, CARD_W - 36)) * (CHIP_H + CHIP_GAP) + 4
    if note:
        h += 18
    return max(h, CARD_H)


def text(x, y, s, size, fill, family=SANS, weight=400, anchor="start", italic=False):
    style = ' font-style="italic"' if italic else ""
    return (f'<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" '
            f'font-weight="{weight}" fill="{fill}" text-anchor="{anchor}"{style}>{s}</text>')


def curve(x1, y1, x2, y2, stroke, width=1.6, dash=""):
    mx = (x1 + x2) / 2
    d = f' stroke-dasharray="{dash}"' if dash else ""
    return (f'<path d="M{x1} {y1} C{mx} {y1} {mx} {y2} {x2 - 7} {y2}" fill="none" '
            f'stroke="{stroke}" stroke-width="{width}" stroke-linecap="round"{d}/>'
            f'<path d="M{x2 - 8} {y2 - 4.5} L{x2} {y2} L{x2 - 8} {y2 + 4.5}" fill="none" '
            f'stroke="{stroke}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"/>')


def render(t):
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" '
           f'role="img" aria-label="clove builds one golden image per repo and makes each clone a '
           f'copy-on-write copy of it">']

    # Column headings.
    for x, label in [(SRC_X + CARD_W / 2, "built from"), (GX + GW / 2, "clove init · clove refresh"),
                     (CL_X + CARD_W / 2, "clove new")]:
        out.append(text(x, 62, label.upper(), 11, t["muted"], weight=600, anchor="middle"))

    # Sources.
    gy_mid = GY + GH / 2
    y = GY
    for src in SOURCES:
        title, line, chips, note = src
        h = source_height(src)
        out.append(f'<rect x="{SRC_X}" y="{y}" width="{CARD_W}" height="{h}" rx="12" '
                   f'fill="{t["card"]}" stroke="{t["card_line"]}"/>')
        out.append(text(SRC_X + 18, y + 26, title, 15, t["ink"], SERIF, 600))
        cy = y + 47
        if line:
            out.append(text(SRC_X + 18, cy, line, 12.5, t["muted"], MONO))
        if chips:
            cy = y + 40
            for row in chip_rows(chips, CARD_W - 36):
                cx = SRC_X + 18
                for c in row:
                    w = chip_w(c)
                    out.append(f'<rect x="{cx}" y="{cy}" width="{w}" height="{CHIP_H}" rx="10" fill="{t["chip"]}"/>')
                    out.append(text(cx + w / 2, cy + 14, c, 11.5, t["chip_ink"], MONO, 600, "middle"))
                    cx += w + CHIP_GAP
                cy += CHIP_H + CHIP_GAP
            cy += 14
        if note:
            out.append(text(SRC_X + 18, cy, note, 12, t["muted"], SANS))
        center = y + h / 2
        out.append(curve(SRC_X + CARD_W, center, GX, gy_mid + (center - gy_mid) * 0.35, t["arrow"]))
        y += h + GAP

    # Golden image: a stack, the front one carrying the contents.
    for off, op in [(16, 0.45), (8, 0.75)]:
        out.append(f'<rect x="{GX + off}" y="{GY - off}" width="{GW}" height="{GH}" rx="16" '
                   f'fill="{t["layer"]}" opacity="{op}"/>')
    out.append(f'<rect x="{GX}" y="{GY}" width="{GW}" height="{GH}" rx="16" fill="{t["golden"]}" '
               f'stroke="{t["golden_line"]}" stroke-opacity="0.55"/>')
    out.append(text(GX + 24, GY + 42, "golden image", 24, t["golden_ink"], SERIF, 600))
    out.append(text(GX + 24, GY + 66, "one per repo, warmed once", 12.5, t["golden_sub"], MONO))
    out.append(f'<line x1="{GX + 24}" y1="{GY + 86}" x2="{GX + GW - 24}" y2="{GY + 86}" '
               f'stroke="{t["golden_sub"]}" stroke-opacity="0.35"/>')
    for i, row in enumerate(GOLDEN_ROWS):
        ry = GY + 124 + i * 44
        out.append(f'<circle cx="{GX + 30}" cy="{ry - 4.5}" r="4" fill="{t["clove"]}"/>')
        out.append(text(GX + 46, ry, row, 14, t["golden_ink"], MONO))

    clone_ys = [gy_mid - CARD_H / 2 + d for d in (-92, 0, 92)]

    # Copy-on-write fan-out.
    out.append(text((GX + GW + 16 + CL_X) / 2, clone_ys[0] - 18, "copy-on-write", 12, t["cow"], MONO, 600, "middle"))
    for y in clone_ys:
        out.append(curve(GX + GW, gy_mid, CL_X, y + CARD_H / 2, t["cow"], 2))

    # Clones: a worktree each, with the clove-gold edge of the mark.
    for (branch, took), y in zip(CLONES, clone_ys):
        out.append(f'<rect x="{CL_X}" y="{y}" width="{CARD_W}" height="{CARD_H}" rx="12" '
                   f'fill="{t["card"]}" stroke="{t["card_line"]}"/>')
        out.append(f'<rect x="{CL_X}" y="{y + 12}" width="4" height="{CARD_H - 24}" rx="2" fill="{t["clove"]}"/>')
        out.append(text(CL_X + 20, y + 27, branch, 14.5, t["ink"], MONO, 600))
        out.append(text(CL_X + 20, y + 47, "checkout, changed steps", 12, t["muted"], SANS))
        out.append(f'<rect x="{CL_X + CARD_W - 52}" y="{y + 14}" width="38" height="20" rx="10" fill="{t["chip"]}"/>')
        out.append(text(CL_X + CARD_W - 33, y + 28, took, 11.5, t["chip_ink"], MONO, 600, "middle"))

    # Footnotes, centred under each column.
    foot_y = GY + GH + 54
    for x, lines in [
        (SRC_X + CARD_W / 2, ["steps are detected from the repo,", "or listed in .clove.toml"]),
        (GX + GW / 2, ["refreshed only when a step's", "inputs change on the base"]),
        (CL_X + CARD_W / 2, ["regular git worktrees,", "isolated, ~6 MB each"]),
    ]:
        for i, line in enumerate(lines):
            out.append(text(x, foot_y + i * 18, line, 13, t["muted"], SERIF, anchor="middle", italic=True))

    out.append("</svg>\n")
    return "\n".join(out)


here = Path(__file__).parent
(here / "how-it-works.svg").write_text(render(THEMES["light"]))
(here / "how-it-works-dark.svg").write_text(render(THEMES["dark"]))
print("wrote how-it-works.svg, how-it-works-dark.svg")
