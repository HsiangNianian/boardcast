# /// script
# dependencies = ["fonttools[woff]==4.62.1", "pillow==12.2.0", "scikit-image==0.26.0"]
# ///
"""uv run scripts/excalifont-strokes.py

Extract Excalifont's ASCII centerlines for the existing animated ink renderer.
The generated data is committed; normal builds do not need Python or these tools.
"""
import io
import json
from pathlib import Path

import numpy as np
from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont
from skimage.measure import approximate_polygon
from skimage.morphology import skeletonize

ROOT = Path(__file__).resolve().parents[1]
SIZE, PAD = 256, 32
FONTS = ROOT / "node_modules/@excalidraw/excalidraw/dist/prod/fonts/Excalifont"


def strokes(image):
    ys, xs = np.nonzero(skeletonize(np.asarray(image) > 127))
    points = set(zip(xs.tolist(), ys.tolist()))
    graph = {}
    for x, y in points:
        graph[x, y] = sorted(
            (x + dx, y + dy)
            for dx in (-1, 0, 1) for dy in (-1, 0, 1)
            if (dx or dy) and (x + dx, y + dy) in points
            and not (dx and dy and ((x + dx, y) in points or (x, y + dy) in points))
        )
    seen, paths = set(), []
    # Endpoints first, junctions next, closed loops last; each edge is drawn once.
    starts = sorted(points, key=lambda p: (len(graph[p]) != 1, len(graph[p]) == 2, p[1], p[0]))
    for start in starts:
        for neighbor in graph[start]:
            if (start, neighbor) in seen:
                continue
            path, prev, cur = [start], start, neighbor
            while True:
                seen.update(((prev, cur), (cur, prev)))
                path.append(cur)
                onward = [p for p in graph[cur] if (cur, p) not in seen]
                if len(graph[cur]) != 2 or not onward:
                    break
                prev, cur = cur, onward[0]
            if len(path) >= 3:
                simple = approximate_polygon(np.array(path), tolerance=0.7)
                paths.append([[round((float(x) - PAD) / SIZE, 4), round((float(y) - PAD) / SIZE, 4)] for x, y in simple])
    return paths


font = next(f for f in (TTFont(p) for p in sorted(FONTS.glob("*.woff2"))) if all(c in f.getBestCmap() for c in range(32, 127)))
assert "SIL OPEN FONT LICENSE" in (ROOT / "public/licenses/handwritten-OFL.txt").read_text()
font.flavor = None
binary = io.BytesIO()
font.save(binary)
face = ImageFont.truetype(io.BytesIO(binary.getvalue()), SIZE)
data = {}
for code in range(32, 127):
    ch = chr(code)
    image = Image.new("L", (SIZE * 2, SIZE * 2))
    ImageDraw.Draw(image).text((PAD, PAD + SIZE * 0.88), ch, font=face, anchor="ls", fill=255)
    paths = strokes(image)
    assert paths or ch == " ", f"Empty glyph: {ch}"
    data[ch] = {"strokes": paths, "advance": round(face.getlength(ch) / SIZE, 4)}

(ROOT / "src/ink/handwritten.json").write_text(json.dumps(data, separators=(",", ":")) + "\n")
print(f"Generated {len(data)} glyphs; license in public/licenses/handwritten-OFL.txt.")
