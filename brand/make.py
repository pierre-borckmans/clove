"""Generates the clove brand SVGs from one geometry. Run: python3 brand/make.py"""
import math, os
os.chdir(os.path.dirname(os.path.abspath(__file__)))

# The c: a thick arc with round ends, open to the right.
CX, CY, R, STROKE, GAP = 50, 50, 31, 19, 42  # GAP: half-opening in degrees
a = math.radians(GAP)
x0, y0 = CX + R * math.cos(a), CY - R * math.sin(a)
x1, y1 = CX + R * math.cos(a), CY + R * math.sin(a)
C_PATH = f"M{x0:.2f} {y0:.2f} A{R} {R} 0 1 0 {x1:.2f} {y1:.2f}"

# The clove: teardrop, tip up, sitting in the counter.
CLOVE = "M50 8 C62 28 74 52 70 76 C68 90 32 90 30 76 C26 52 38 28 50 8Z"
CLOVE_TF = "translate(53 51) rotate(16) scale(0.33) translate(-50 -49)"

def mark(c, clove):
    return (f'<path d="{C_PATH}" fill="none" stroke="{c}" stroke-width="{STROKE}" stroke-linecap="round"/>'
            f'<path d="{CLOVE}" transform="{CLOVE_TF}" fill="{clove}"/>')

INK, PAPER, RUST, RUST_HI, RUST_LO = "#1a1714", "#f6f1e7", "#b4532a", "#c8693c", "#a2471f"
def svg(body, vb="0 0 100 100"):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}">{body}</svg>\n'

open("mark.svg", "w").write(svg(f"<title>clove</title>{mark(INK, RUST)}"))
open("mark-on-dark.svg", "w").write(svg(f"<title>clove</title>{mark(PAPER, RUST)}"))
# App icon on Apple's 1024 grid: 824 tile, mark about 62% of the tile.
open("icon.svg", "w").write(svg(f'''<defs>
<linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{RUST_HI}"/><stop offset="1" stop-color="{RUST_LO}"/></linearGradient>
<linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".16"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>
<rect x="100" y="100" width="824" height="824" rx="185" fill="url(#g)"/>
<rect x="100" y="100" width="824" height="824" rx="185" fill="url(#s)"/>
<g transform="translate(232 232) scale(5.6)">{mark(PAPER, INK)}</g>''', "0 0 1024 1024"))
open("favicon.svg", "w").write(svg(f'<title>clove</title><rect width="100" height="100" rx="22" fill="{RUST}"/><g transform="translate(9 9) scale(.82)">{mark(PAPER, INK)}</g>'))
# Menu-bar template image: alpha only.
open("tray.svg", "w").write(svg(mark("#000", "#000")))
print(C_PATH)
