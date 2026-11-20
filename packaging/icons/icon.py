#!/usr/bin/env python3
"""Write the app icon as SVG: the Banshu mark on Apple's continuous-corner tile.

    icon.py macos    > macos.svg     Big Sur grid: 824 px tile on a 1024 canvas, with its drop shadow
    icon.py desktop  > desktop.svg   Linux and Windows: a larger tile, no shadow
    icon.py ios      > ios.svg       App Store: full-bleed square, the system masks the corners
    icon.py favicon                  The website's tab icon: the desktop tile, outlined in a dark browser

make.sh turns these into icon.icns, the Linux PNG and icon.ico.
"""
import math
import sys

INK, CHALK, RED = "#1e1e1e", "#f3efe6", "#e8432a"
# The mark, drawn on a 60-unit tile whose top-left corner sits at (2, 2); see promo/mockup/Brand.html.
WAVE = "M12 33 C14.5 23 16.5 23 19 33 S23.5 44 26 33 S29.5 27 32 33 S35 37 38 34 C42 31.5 46 33.5 50 33"
DOT = (52.5, 33, 4)
# The mark spans x 9.7-56.5 and y 23-44 with its stroke, so it sits right of and below the tile's centre
# (32, 32). Icons keep it inside Apple's grid: centred and scaled to about 80% of the tile.
GLYPH = "translate(32 32) scale(0.8) translate(-33.1 -33.5)"


def squircle(x, y, size, radius, steps=48):
    """A square with Apple's continuous corners, as an SVG path.

    Each corner is a superellipse quadrant that starts 1.528 x radius from the corner (where Apple's
    corner curvature begins), with its exponent chosen so the curve crosses the corner's diagonal
    where a circular corner of the same radius would. That reproduces the icon template's outline
    to within a pixel at 1024 px.
    """
    reach = 1.528665 * radius
    n = math.log(2) / -math.log(1 - (math.sqrt(2) - 1) * radius / (math.sqrt(2) * reach))
    pts = []
    corners = [  # corner centre of each quadrant, and the angle range it sweeps
        (x + size - reach, y + reach, -90, 0),
        (x + size - reach, y + size - reach, 0, 90),
        (x + reach, y + size - reach, 90, 180),
        (x + reach, y + reach, 180, 270),
    ]
    for cx, cy, a0, a1 in corners:
        for i in range(steps + 1):
            t = math.radians(a0 + (a1 - a0) * i / steps)
            c, s = math.cos(t), math.sin(t)
            px = cx + reach * math.copysign(abs(c) ** (2 / n), c)
            py = cy + reach * math.copysign(abs(s) ** (2 / n), s)
            pts.append(f"{px:.2f} {py:.2f}")
    return "M" + " L".join(pts) + " Z"


def svg(kind):
    canvas = 1024
    if kind == "macos":
        tile, radius, shadow = 824, 185.4, True
    elif kind in ("desktop", "favicon"):
        tile, radius, shadow = 928, 208.8, False
    elif kind == "ios":
        tile, radius, shadow = 1024, 0, False
    else:
        sys.exit(f"unknown icon kind {kind!r}: macos, desktop or ios")
    off = (canvas - tile) / 2
    scale = tile / 60
    shape = f'<rect width="{canvas}" height="{canvas}"' if radius == 0 else f'<path d="{squircle(off, off, tile, radius)}"'
    # The template's shadow, as a blurred copy of the tile underneath it: a filter on the tile itself
    # would also run over its gradient and band it.
    under = ""
    if shadow:
        under = ('<filter id="s" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="10"/></filter>'
                 f'{shape} fill="#000" fill-opacity="0.3" transform="translate(0 10)" filter="url(#s)"/>\n')
    # A dark tab strip is about the tile's own colour, so there the tab icon gets a faint chalk edge.
    if kind == "favicon":
        under += ("<style>@media (prefers-color-scheme: dark) "
                  "{ .t { stroke: #f3efe6; stroke-opacity: 0.3; stroke-width: 40 } }</style>\n")
    cx, cy, r = DOT
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {canvas} {canvas}" width="{canvas}" height="{canvas}">
{under}<linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2a2a29"/><stop offset="1" stop-color="{INK}"/></linearGradient>
{shape} class="t" fill="url(#g)"/>
<g transform="translate({off - 2 * scale:.3f} {off - 2 * scale:.3f}) scale({scale:.5f}) {GLYPH}">
<path d="{WAVE}" fill="none" stroke="{CHALK}" stroke-width="5.2" stroke-linecap="round" stroke-linejoin="round"/>
<circle cx="{cx}" cy="{cy}" r="{r}" fill="{RED}"/>
</g>
</svg>
"""


if __name__ == "__main__":
    sys.stdout.write(svg(sys.argv[1] if len(sys.argv) > 1 else "macos"))
