#!/usr/bin/env python3
"""Write the app icon as SVG: the Alcyo kingfisher on Apple's continuous-corner tile.

    icon.py macos    > macos.svg     Big Sur grid: 824 px tile on a 1024 canvas, with its drop shadow
    icon.py desktop  > desktop.svg   Linux and Windows: a larger tile, no shadow
    icon.py ios      > ios.svg       App Store: full-bleed square, the system masks the corners
    icon.py favicon                  The website's tab icon: the desktop tile, outlined in a dark browser

make.sh turns these into icon.icns, the Linux PNG and icon.ico.
"""
import math
import sys

INK, CHALK, TEAL, ORANGE = "#1e1e1e", "#f3efe6", "#0a7ea4", "#f07a2b"
# The kingfisher, drawn on a 512 canvas as calligraphy: one stroke from the forked tail along the back into
# the round head and out to the beak, a slim orange crescent for the breast with a hairline of paper between,
# a wing and a long eye cut out of the body. The working files are in promo/mockup/halcy-final.
BODY = "M48 430 C120 392 168 300 214 236 C240 182 280 134 334 132 C370 131 394 150 398 172 L480 194 L390 204 C374 226 348 244 316 262 C262 292 214 330 168 372 C134 402 102 426 70 448 L84 429 Z"
BREAST = "M372 236 C350 274 300 318 246 352 C214 372 182 390 148 402 C196 404 252 386 298 352 C336 324 362 284 372 236 Z"
CUTS = ("M150 372 C190 318 232 262 290 222 C246 270 206 320 168 370 Z",
        "M318 176 C330 167 352 163 370 168 C354 175 336 179 318 176 Z")
# The bird spans x 48-480, y 131-448 on that canvas. Icons centre it and scale it to about 74% of the tile.
CENTRE, SPAN, FILL = (264, 289.5), 432, 0.74


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
    shape = f'<rect width="{canvas}" height="{canvas}"' if radius == 0 else f'<path d="{squircle(off, off, tile, radius)}"'
    # The template's shadow, as a blurred copy of the tile underneath it: a filter on the tile itself
    # would also run over its gradient and band it.
    under = ""
    if shadow:
        under = ('<filter id="s" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="10"/></filter>'
                 f'{shape} fill="#000" fill-opacity="0.3" transform="translate(0 10)" filter="url(#s)"/>\n')
    # A light tab strip is about the tile's own colour, so there the tab icon gets a faint ink edge.
    if kind == "favicon":
        under += ("<style>@media (prefers-color-scheme: light) "
                  "{ .t { stroke: #1e1e1e; stroke-opacity: 0.14; stroke-width: 24 } }</style>\n")
    k = tile * FILL / SPAN
    cuts = "".join(f'<path d="{c}" fill="#000"/>' for c in CUTS)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {canvas} {canvas}" width="{canvas}" height="{canvas}">
{under}<linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f8f5ef"/><stop offset="1" stop-color="#ebe5d8"/></linearGradient>
<mask id="m"><rect x="-100" y="-100" width="800" height="800" fill="#fff"/>{cuts}</mask>
{shape} class="t" fill="url(#g)"/>
<g transform="translate({canvas / 2} {canvas / 2}) scale({k:.5f}) translate({-CENTRE[0]} {-CENTRE[1]})">
<path d="{BODY}" fill="{TEAL}" mask="url(#m)"/>
<path d="{BREAST}" fill="{ORANGE}"/>
</g>
</svg>
"""


if __name__ == "__main__":
    sys.stdout.write(svg(sys.argv[1] if len(sys.argv) > 1 else "macos"))
