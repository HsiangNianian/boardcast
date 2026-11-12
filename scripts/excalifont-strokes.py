# /// script
# dependencies = ["fonttools[woff]==4.62.1", "pillow==12.2.0", "scikit-image==0.26.0"]
# ///
"""uv run scripts/excalifont-strokes.py

Reads Excalifont from the pinned @excalidraw/excalidraw 0.18.1 package on npm, downloaded and checked here.

Extract ASCII and mathematical centerlines for the existing animated ink renderer.
The generated data is committed; normal builds do not need Python or these tools.

A skeleton breaks at every junction and runs in no particular direction, so the pieces are then put back
together the way a hand writes them: a line going straight through a junction is one stroke, strokes run
left to right and top to bottom, loops go round as a pen does, and a stroke that starts on another comes
after it, with dots last.
"""
import base64
import hashlib
import io
import itertools
import json
import math
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


# Ends closer than NEAR meet; a piece shorter than SPUR hanging off a junction, or than BRIDGE between two,
# is a skeleton artefact; a stroke shorter than DOT is a dot.
NEAR, SPUR, BRIDGE, DOT = 0.035, 0.05, 0.07, 0.1


def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def length(p):
    return sum(dist(a, b) for a, b in zip(p, p[1:]))


def heading(p):
    """The unit vector from a stroke's first point into it, judged over its first stretch."""
    walked, b = 0.0, p[-1]
    for q, r in zip(p, p[1:]):
        walked += dist(q, r)
        if walked >= 0.15:
            b = r
            break
    d = dist(p[0], b) or 1.0
    return ((b[0] - p[0][0]) / d, (b[1] - p[0][1]) / d)


def to_line(c, p):
    """The distance from a point to a polyline."""
    best = math.inf
    for a, b in zip(p, p[1:]):
        dx, dy = b[0] - a[0], b[1] - a[1]
        t = max(0.0, min(1.0, ((c[0] - a[0]) * dx + (c[1] - a[1]) * dy) / (dx * dx + dy * dy or 1.0)))
        best = min(best, dist(c, (a[0] + dx * t, a[1] + dy * t)))
    return best


def ends(paths):
    return [(i, e, p[0] if e == 0 else p[-1]) for i, p in enumerate(paths) for e in (0, 1)]


def meeting(paths, c):
    return sum(dist(c, q) <= NEAR for _, _, q in ends(paths))


def prune(paths):
    """Drops the short spikes a skeleton leaves where strokes meet; a stroke on its own (a dot) stays."""
    keep = []
    for p in paths:
        a, b = meeting(paths, p[0]), meeting(paths, p[-1])
        if length(p) < SPUR and max(a, b) >= 3 and min(a, b) == 1:
            continue
        keep.append(p)
    return keep


def contract(paths):
    """A skeleton splits a crossing into two forks a short bridge apart (X, K, A); the bridge goes, and its
    two forks become one junction at its middle."""
    while True:
        for k, p in enumerate(paths):
            if length(p) < BRIDGE and meeting(paths, p[0]) >= 3 and meeting(paths, p[-1]) >= 3:
                m = [round((p[0][0] + p[-1][0]) / 2, 4), round((p[0][1] + p[-1][1]) / 2, 4)]
                rest = []
                for q in paths[:k] + paths[k + 1:]:
                    q = list(q)
                    for e in (0, -1):
                        if min(dist(q[e], p[0]), dist(q[e], p[-1])) <= NEAR:
                            q[e] = m
                    rest.append(q)
                paths = rest
                break
        else:
            return paths


def join(paths):
    """Joins the pieces a junction cut apart: the two ends of a bend always, and of a crossing the pair that
    goes straight through, straightest first."""
    paths = [list(p) for p in paths]
    while True:
        best = None
        for (i, e, a), (j, f, b) in itertools.combinations(ends(paths), 2):
            if i == j or dist(a, b) > NEAR:
                continue
            (ax, ay), (bx, by) = heading(paths[i] if e == 0 else paths[i][::-1]), heading(paths[j] if f == 0 else paths[j][::-1])
            straight = -(ax * bx + ay * by)
            if (meeting(paths, a) == 2 or straight > 0.75) and (best is None or straight > best[0]):
                best = (straight, i, e, j, f)
        if best is None:
            return paths
        _, i, e, j, f = best
        a = paths[i][::-1] if e == 0 else paths[i]
        b = paths[j] if f == 0 else paths[j][::-1]
        mid = [round((a[-1][0] + b[0][0]) / 2, 4), round((a[-1][1] + b[0][1]) / 2, 4)]
        paths = [p for k, p in enumerate(paths) if k not in (i, j)] + [a[:-1] + [mid] + b[1:]]


def closed(p):
    return len(p) > 3 and dist(p[0], p[-1]) <= NEAR * 1.5 and length(p) > 0.2


def centre(p):
    return (sum(q[0] for q in p) / len(p), sum(q[1] for q in p) / len(p))


def clockwise(p):
    """On screen, with y down."""
    return sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(p, p[1:] + p[:1])) > 0


def rank(c):
    """Earlier for a point higher up, and then further left."""
    return c[1] + 0.5 * c[0]


def hangs(p, strokes):
    """Whether a loop starts on another stroke."""
    return closed(p) and any(to_line(p[0], q) <= NEAR for q in strokes if q is not p)


def walk(p, k, step):
    """The point about 0.06 from p[k] along the stroke, one way or the other."""
    walked, j = 0.0, k
    while 0 <= j + step < len(p) and walked < 0.06:
        walked += dist(p[j], p[j + step])
        j += step
    return p[j]


def split(p):
    """A stroke whose highest-left point is a sharp corner inside it (E, F, R, 5) is two strokes from that
    corner: the hand starts there, not at either end."""
    if closed(p) or len(p) < 3:
        return [p]
    k = min(range(len(p)), key=lambda k: rank(p[k]))
    if min(length(p[: k + 1]), length(p[k:])) < 0.08 or rank(p[k]) > min(rank(p[0]), rank(p[-1])) - 0.1:
        return [p]
    a, b = walk(p, k, -1), walk(p, k, 1)
    u, v = (a[0] - p[k][0], a[1] - p[k][1]), (b[0] - p[k][0], b[1] - p[k][1])
    cos = (u[0] * v[0] + u[1] * v[1]) / ((math.hypot(*u) * math.hypot(*v)) or 1.0)
    return [p[k::-1], p[k:]] if cos > -0.5 else [p]


def orient(p, strokes):
    if not closed(p):
        return p if rank(p[0]) <= rank(p[-1]) else p[::-1]
    cx, cy = centre(p[:-1])
    if not hangs(p, strokes):
        # A loop on its own (o, 0) starts near the top and goes anticlockwise.
        ring = p[:-1]
        k = max(range(len(ring)), key=lambda k: 0.5 * (ring[k][0] - cx) - (ring[k][1] - cy))
        p = ring[k:] + ring[:k] + [ring[k]]
        return p[::-1] if clockwise(p) else p
    # A bowl on a stem starts where it meets the stem: clockwise when the stem is on its left (b, p, D),
    # anticlockwise when it is on its right (a, d, q).
    want = p[0][0] < cx
    return p if clockwise(p) == want else p[::-1]


def crosses(p, q):
    def cut(a, b, c, d):
        def side(o, s, t):
            return (s[0] - o[0]) * (t[1] - o[1]) - (s[1] - o[1]) * (t[0] - o[0])
        return side(a, b, c) * side(a, b, d) < 0 and side(c, d, a) * side(c, d, b) < 0
    return any(cut(a, b, c, d) for a, b in zip(p, p[1:]) for c, d in zip(q, q[1:]))


def flat(p):
    return abs(p[-1][0] - p[0][0]) > 2 * abs(p[-1][1] - p[0][1])


def after(p, q, strokes):
    """Whether p is written after q."""
    if hangs(p, strokes) and p[0][0] > centre(p[:-1])[0]:
        return False  # a, d, q: the bowl first, then the stem
    if hangs(q, strokes) and q[0][0] > centre(q[:-1])[0]:
        return to_line(q[0], p) <= NEAR
    starts_on = to_line(p[0], q) <= NEAR and min(dist(p[0], q[0]), dist(p[0], q[-1])) > NEAR
    return starts_on or (crosses(p, q) and flat(p) and not flat(q))


def order(paths, letter):
    # A letter's dots go last (i, j, ä); elsewhere they are written in their place (:, ;, !).
    body = letter and any(length(p) > 0.2 for p in paths)
    dots = [p for p in paths if body and length(p) < DOT]
    rest = [p for p in paths if not (body and length(p) < DOT)]
    done = []
    while rest:
        ready = [p for p in rest if not any(q is not p and after(p, q, paths) for q in rest)] or rest
        # From one point, the stroke going down first (E, F, R: the stem, then the bar or bowl).
        first = min(ready, key=lambda p: rank(p[0]) - 0.05 * heading(p)[1])
        done.append(first)
        rest = [p for p in rest if p is not first]
    return done + sorted(dots, key=lambda p: rank(p[0]))


def tidy(paths, letter):
    """Strokes as a hand writes them: whole, in a hand's direction and order."""
    paths = [q for p in join(prune(contract(paths))) for q in split(p)]
    paths = [orient(p, paths) for p in paths]
    return order(paths, letter)


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
            data[ch] = {"strokes": tidy(paths, unicodedata.category(ch).startswith("L")), "advance": round(face.getlength(ch) / SIZE, 4)}

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
