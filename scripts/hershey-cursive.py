"""python3 scripts/hershey-cursive.py

Cursive lowercase for a messy hand, from the Hershey script font ("scripts") in the pinned hersheytext 2.0.0
package on npm, downloaded and checked here. Writes assets/glyphs/cursive.json and the package's license.

The letters are scaled to the print letters' x-height and baseline (assets/glyphs/handwritten.json), so a
cursive word is as tall as a printed one, and each advance is the letter's own width, so the letters of a
word sit close enough to join. The generated data is committed; normal builds do not need Python.
"""
import base64
import hashlib
import io
import json
import re
import tarfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "https://registry.npmjs.org/hersheytext/-/hersheytext-2.0.0.tgz"
PACKAGE_SHA512 = "t8UPIWKpPKffT7QcGG8AWXen4zVpfMdlO5uq3cE8aNQGFxSEV/HoGAIFki6lioNi35WG73gaeW4VY3avxsJIrA=="
# In the print letters: the baseline, and the top of a letter without ascender (x, o, n, z, about 0.34),
# which a joined-up letter just passes, as its loops make it look smaller.
BASELINE, X_HEIGHT = 0.86, 0.36
# In Hershey units: the baseline and the top of the lowercase.
H_BASELINE, H_X_TOP = 22.0, 8.0
GAP = 0.03

data = urllib.request.urlopen(PACKAGE, timeout=120).read()
assert base64.b64encode(hashlib.sha512(data).digest()).decode() == PACKAGE_SHA512, "Unexpected hersheytext package"
with tarfile.open(fileobj=io.BytesIO(data)) as tar:
    fonts = json.load(tar.extractfile("package/hersheytext.min.json"))
    license_text = tar.extractfile("package/LICENSE").read().decode()

scale = X_HEIGHT / (H_BASELINE - H_X_TOP)
out = {}
for code in range(ord("a"), ord("z") + 1):
    path = fonts["scripts"]["chars"][code - 33]["d"]
    strokes = []
    for part in path.split("M")[1:]:
        points = [tuple(map(float, xy.split(","))) for xy in re.split(r"[\sL]+", part.strip()) if "," in xy]
        if len(points) >= 2:
            strokes.append([[x * scale, BASELINE - (H_BASELINE - y) * scale] for x, y in points])
    # The plotter dotted an i or a j first; a hand dots it last.
    dot = lambda s: sum(((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) ** 0.5 for a, b in zip(s, s[1:])) < 0.08
    strokes = [s for s in strokes if not dot(s)] + [s for s in strokes if dot(s)]
    left = min(x for s in strokes for x, _ in s)
    right = max(x for s in strokes for x, _ in s)
    shift = GAP - left
    out[chr(code)] = {
        "strokes": [[[round(x + shift, 4), round(y, 4)] for x, y in s] for s in strokes],
        "advance": round(right - left + 2 * GAP, 4),
    }

(ROOT / "assets/glyphs/cursive.json").write_text(json.dumps(out, separators=(",", ":")) + "\n")
(ROOT / "assets/licenses/hersheytext-MIT.txt").write_text("Source: hersheytext 2.0.0, hersheytext.min.json (scripts)\n\n" + license_text)
print(f"Wrote {len(out)} cursive letters to assets/glyphs/cursive.json")
