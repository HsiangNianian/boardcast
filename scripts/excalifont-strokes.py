# /// script
# dependencies = ["fonttools[woff]==4.62.1", "pillow==12.2.0", "scikit-image==0.26.0"]
# ///
"""uv run scripts/excalifont-strokes.py

Reads Excalifont from the pinned @excalidraw/excalidraw 0.18.1 package on npm, downloaded and checked here.

Extract ASCII and mathematical centerlines for the existing animated ink renderer.
The generated data is committed; normal builds do not need Python or these tools.
"""
import base64
import hashlib
import io
import json
import tarfile
import unicodedata
import urllib.request
from pathlib import Path

import numpy as np
from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont
from skimage.measure import approximate_polygon
from skimage.morphology import skeletonize

ROOT = Path(__file__).resolve().parents[1]
SIZE, PAD = 256, 32
PACKAGE = "https://registry.npmjs.org/@excalidraw/excalidraw/-/excalidraw-0.18.1.tgz"
PACKAGE_SHA512 = "6i5Gt7IDTOH//qa0Z315Ly5iVRhjWpu2whrlQFqkuwrkKUWgRsMk0P5qdE7bpyDpai7jeLeWYkyj1eVAfni1lw=="


def fonts():
    """The package's font folder, unpacked once into target/cache."""
    out = ROOT / "target/cache/excalidraw-0.18.1"
    if not (out / "package/dist/prod/fonts").is_dir():
        data = urllib.request.urlopen(PACKAGE, timeout=120).read()
        assert base64.b64encode(hashlib.sha512(data).digest()).decode() == PACKAGE_SHA512, "Unexpected Excalidraw package"
        with tarfile.open(fileobj=io.BytesIO(data)) as tar:
            tar.extractall(out, members=[m for m in tar.getmembers() if m.name.startswith("package/dist/prod/fonts/")], filter="data")
    return out / "package/dist/prod/fonts"


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
    seen, paths, dots = set(), [], []
    # Endpoints first, junctions next, closed loops last; each edge is drawn once.
    starts = sorted(points, key=lambda p: (len(graph[p]) != 1, len(graph[p]) == 2, p[1], p[0]))
    for start in starts:
        if not graph[start]:
            # A round dot can skeletonize to one pixel; keep it as a short stroke.
            x, y = ((v - PAD) / SIZE for v in start)
            dots.append([[round(x, 4), round(y, 4)], [round(x + 1 / SIZE, 4), round(y, 4)]])
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
            if len(path) >= 2:
                simple = approximate_polygon(np.array(path), tolerance=0.7)
                (paths if len(path) >= 3 else dots).append([[round((float(x) - PAD) / SIZE, 4), round((float(y) - PAD) / SIZE, 4)] for x, y in simple])
    return paths or dots


assert "SIL OPEN FONT LICENSE" in (ROOT / "assets/licenses/handwritten-OFL.txt").read_text()
assert "Microsoft Corporation" in (ROOT / "assets/licenses/handwritten-cascadia.txt").read_text()
codes = set(range(32, 127)) | {0xb0, 0xb1, 0xb2, 0xb3, 0xb5, 0xb9, 0xbc, 0xbd, 0xbe, 0xd7, 0xf7}
codes |= {c for c in range(0x370, 0x400) if unicodedata.category(chr(c)).startswith("L")}
codes |= set(range(0x2070, 0x20a0)) | set(range(0x2100, 0x2150)) | set(range(0x2190, 0x2300))
codes |= set(range(0x1d62, 0x1d6b))
data = {}
# shortcut: symbols are limited to the bundled fonts; broader alphabets need another licensed glyph source.
for family in ["Excalifont", "Cascadia"]:
    for path in sorted((fonts() / family).glob("*.woff2")):
        font = TTFont(path)
        available = sorted(c for c in codes & font.getBestCmap().keys() if chr(c) not in data)
        if not available:
            continue
        font.flavor = None
        binary = io.BytesIO()
        font.save(binary)
        face = ImageFont.truetype(io.BytesIO(binary.getvalue()), SIZE)
        for code in available:
            ch = chr(code)
            image = Image.new("L", (SIZE * 2, SIZE * 2))
            ImageDraw.Draw(image).text((PAD, PAD + SIZE * 0.88), ch, font=face, anchor="ls", fill=255)
            paths = strokes(image)
            assert paths or ch == " ", f"Empty glyph: {ch}"
            data[ch] = {"strokes": paths, "advance": round(face.getlength(ch) / SIZE, 4)}

for code in sorted(codes):
    ch = chr(code)
    if ch in data:
        continue
    placement = unicodedata.decomposition(ch).split()[:1]
    base = data.get(unicodedata.normalize("NFKD", ch))
    if base and placement in [["<super>"], ["<sub>"]]:
        shift = -0.15 if placement == ["<super>"] else 0.4
        data[ch] = {"strokes": [[[round(x * 0.65, 4), round(y * 0.65 + shift, 4)] for x, y in s] for s in base["strokes"]], "advance": round(base["advance"] * 0.65, 4)}

(ROOT / "assets/glyphs/handwritten.json").write_text(json.dumps(dict(sorted(data.items())), separators=(",", ":")) + "\n")
# The website's pen only writes ASCII, so it carries just that part.
ascii_only = {chr(c): data[chr(c)] for c in range(32, 127) if chr(c) in data}
(ROOT / "crates/site/src/excalifont.json").write_text(json.dumps(ascii_only, separators=(",", ":")) + "\n")
print(f"Generated {len(data)} glyphs; licenses in assets/licenses/handwritten-*.txt.")
