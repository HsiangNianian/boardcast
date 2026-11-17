# /// script
# dependencies = ["fonttools[woff]==4.62.1", "pillow==12.2.0", "scikit-image==0.26.0"]
# ///
"""uv run scripts/xiaolai-hangul.py

Reads Xiaolai from the pinned @excalidraw/excalidraw 0.18.1 package on npm, as excalifont-strokes.py does.

Extract Korean centerlines: every Hangul syllable and the letters on their own (ㄱ, ㅏ, ㅋㅋ).
The generated data is committed; normal builds do not need Python or these tools.

The centerlines come out of the same skeleton as the English ones. What differs is the order a hand writes
them in: a syllable is its first consonant, then its vowel, then the consonant under it, and which of those
a stroke belongs to is told from where the vowel's long strokes are.
"""
import gzip
import importlib.util
import json
import sys
from pathlib import Path

import numpy as np
from skimage.measure import approximate_polygon

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("excalifont_strokes", Path(__file__).with_name("excalifont-strokes.py"))
pen = importlib.util.module_from_spec(spec)
sys.dont_write_bytecode = True
spec.loader.exec_module(pen)

# Vowels by where they sit: beside the first consonant, under it, or both. Numbered as Unicode orders them.
BESIDE = {0: (1, 1), 1: (2, 1), 2: (1, 2), 3: (2, 2), 4: (1, 1), 5: (2, 1), 6: (1, 2), 7: (2, 2), 20: (1, 0)}
UNDER = {8: 1, 12: 2, 13: 1, 17: 2, 18: 0}
# A vowel that is both: the one under, then the one beside.
BOTH = {9: (8, 0), 10: (8, 1), 11: (8, 20), 14: (13, 4), 15: (13, 5), 16: (13, 20), 19: (18, 20)}
# Consonants that are two side by side, first and last.
DOUBLE_FIRST = {1, 4, 8, 10, 13}
DOUBLE_LAST = {2, 3, 5, 6, 9, 10, 11, 12, 13, 14, 15, 18, 20}

TOUCH = 0.05


def box(p):
    xs, ys = [q[0] for q in p], [q[1] for q in p]
    return min(xs), min(ys), max(xs), max(ys)


def straight(p):
    return pen.length(p) < 1.15 * pen.dist(p[0], p[-1])


def upright(p):
    x0, y0, x1, y1 = box(p)
    return straight(p) and (y1 - y0) > 2.5 * (x1 - x0)


def level(p):
    x0, y0, x1, y1 = box(p)
    return straight(p) and (x1 - x0) > 2.5 * (y1 - y0)


def middle(p):
    x0, y0, x1, y1 = box(p)
    return (x0 + x1) / 2, (y0 + y1) / 2


def down(p):
    """The longest stretch of a stroke that runs straight down: how tall it is, where it starts, and where
    it is across, and which of its points it is. A vowel's long stroke can end in a hook (ㅝ over a
    consonant), so it is not always straight."""
    best = (0.0, 0.0, 0.0, 0, 0)
    for i in range(len(p)):
        j = i
        while j + 1 < len(p) and abs(p[j + 1][0] - p[i][0]) <= 0.04:
            j += 1
        ys = [q[1] for q in p[i : j + 1]]
        if max(ys) - min(ys) > best[0]:
            best = (max(ys) - min(ys), min(ys), sum(q[0] for q in p[i : j + 1]) / (j + 1 - i), i, j)
    return best


def squared(p):
    """A closed loop that is a box (ㅁ) is three strokes: the left side, the top and right, the bottom."""
    if not pen.closed(p):
        return [p]
    ring = p[:-1]
    x0, y0, x1, y1 = box(ring)
    size = min(x1 - x0, y1 - y0)
    wanted = [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
    corners = [min(range(len(ring)), key=lambda k: pen.dist(ring[k], c)) for c in wanted]
    # A ring (ㅇ) comes nowhere near the corners of the box around it.
    if size < 0.1 or len(set(corners)) < 4 or any(pen.dist(ring[k], c) > 0.12 * size for k, c in zip(corners, wanted)):
        return [p]
    top_left = corners[0]
    ring = ring[top_left:] + ring[:top_left]
    if pen.clockwise(ring + ring[:1]):
        ring = ring[:1] + ring[:0:-1]
    # Now the ring runs from the top left down the left side, along the bottom and up the right.
    at = [min(range(len(ring)), key=lambda k: pen.dist(ring[k], c)) for c in [(x0, y1), (x1, y1)]]
    if not 0 < at[0] < at[1] < len(ring):
        return [p]
    return [ring[: at[0] + 1], (ring[at[1]:] + ring[:1])[::-1], ring[at[0] : at[1] + 1]]


def no_climb(p):
    """A stroke that ends going up a side (the right of ㅂ) is two: a hand lifts and comes down it."""
    if pen.closed(p) or len(p) < 3:
        return [p]
    climbed, k = 0.0, len(p) - 1
    while k > 0 and (p[k - 1][1] - p[k][1] > 2 * abs(p[k][0] - p[k - 1][0]) or pen.dist(p[k], p[k - 1]) < 0.015):
        climbed += p[k - 1][1] - p[k][1]
        k -= 1
    if climbed < 0.12 or k == 0 or pen.length(p[: k + 1]) < 0.1:
        return [p]
    return [p[: k + 1], p[k:][::-1]]


def by_hand(strokes):
    """One consonant's strokes in a hand's order: from the top left, a stroke that starts on another after it."""
    rest, done = list(strokes), []
    while rest:
        def waits(p):
            return any(q is not p and pen.to_line(p[0], q) <= pen.NEAR and min(pen.dist(p[0], q[0]), pen.dist(p[0], q[-1])) > pen.NEAR for q in rest)
        ready = [p for p in rest if not waits(p)] or rest
        first = min(ready, key=lambda p: p[0][1] + 0.25 * p[0][0])
        done.append(first)
        rest = [p for p in rest if p is not first]
    return done


def consonant(strokes, double):
    """A doubled consonant is its left one, then its right one."""
    if double and len(strokes) > 1:
        across = sorted(strokes, key=lambda p: middle(p)[0])
        gaps = [(min(box(p)[0] for p in across[k:]) - max(box(p)[2] for p in across[:k]), k) for k in range(1, len(across))]
        gap, k = max(gaps)
        if gap > -0.02:
            return by_hand(across[:k]) + by_hand(across[k:])
    return by_hand(strokes)


def ticks(strokes, long, count, flat):
    """The short strokes that stand on a vowel's long one."""
    found = [p for p in strokes if (pen.length(p) < 0.26 and level(p) if flat else pen.length(p) < 0.42 and upright(p)) and min(pen.to_line(p[0], long), pen.to_line(p[-1], long)) <= TOUCH]
    return sorted(found, key=pen.length)[:count]


def syllable(strokes, vowel, last):
    """A syllable's strokes as its first consonant's, its vowel's and its last consonant's. Returns None when
    the vowel is not found."""
    under, beside = BOTH.get(vowel, (vowel if vowel in UNDER else None, vowel if vowel in BESIDE else None))
    rest = list(strokes)
    stems, written_under, marks, final = [], [], [], []
    bottom = height = 0.0

    def take(found):
        nonlocal rest
        rest = [p for p in rest if not any(p is q for q in found)]

    if beside is not None:
        tall = 0.25 if last else 0.35
        stems = [p for p in rest if down(p)[0] >= tall and down(p)[1] < 0.35 and down(p)[2] > 0.45]
        stems = sorted(stems, key=lambda p: -down(p)[2])[: BESIDE[beside][0]]
        if len(stems) < BESIDE[beside][0]:
            return None
        take(stems)
        # A long stroke that runs on into the next (the bar of ㅚ, or the consonant under it) ends where it
        # stops going down; what is left is a stroke of its own.
        for k, p in enumerate(stems):
            _, _, _, i, j = down(p)
            if pen.length(p) - down(p)[0] >= 0.25:
                if i > 0 and j < len(p) - 1:
                    return None
                stems[k] = p[i : j + 1]
                rest.append(p[j:] if i == 0 else p[: i + 1])
        bottom = max(box(p)[3] for p in stems)
    if under is not None:
        # Beside a long stroke the bar is short and the lowest thing above the last consonant; alone it is
        # the widest stroke there is.
        if beside is not None:
            bars = [p for p in rest if level(p) and box(p)[2] - box(p)[0] >= 0.3 and (not last or middle(p)[1] < bottom + 0.08)]
            bar = max(bars, key=lambda p: middle(p)[1], default=None)
        else:
            bars = [p for p in rest if level(p) and box(p)[2] - box(p)[0] >= 0.5]
            bar = max(bars, key=lambda p: box(p)[2] - box(p)[0], default=None)
        if bar is None:
            return None
        take([bar])
        mine = ticks(rest, bar, UNDER[under], False)
        take(mine)
        height = middle(bar)[1]
        above = sorted((p for p in mine if middle(p)[1] < height), key=lambda p: middle(p)[0])
        below = sorted((p for p in mine if middle(p)[1] >= height), key=lambda p: middle(p)[0])
        written_under = above + [bar] + below
    if last:
        if beside is None:
            final = [p for p in rest if middle(p)[1] > height]
        else:
            final = [p for p in rest if box(p)[1] > max(bottom - 0.12, height + 0.005)]
        take(final)
        if not final:
            return None
    for stem in stems:
        mine = ticks(rest, stem, BESIDE[beside][1] - len(marks), True)
        take(mine)
        marks += mine
    # Left to right: the tick of ㅓ before its long stroke, the tick of ㅏ after it.
    across = sorted([(down(p)[2], middle(p)[1], p) for p in stems] + [(middle(p)[0], middle(p)[1], p) for p in marks], key=lambda t: (round(t[0], 2), t[1]))
    return rest, written_under + [p for _, _, p in across], final


def ringed(paths):
    """Where a ring touches two strokes (요), a skeleton runs one stroke up the first, round most of the ring
    and down the second, leaving the rest of the ring on its own. The ring is closed again and the two
    strokes cut from it."""
    for b in paths:
        for a in paths:
            if a is b or pen.closed(a) or pen.closed(b):
                continue
            i = min(range(len(a)), key=lambda k: pen.dist(a[k], b[0]))
            j = min(range(len(a)), key=lambda k: pen.dist(a[k], b[-1]))
            if max(pen.dist(a[i], b[0]), pen.dist(a[j], b[-1])) > pen.NEAR or i == j:
                continue
            i, j, chord = (i, j, b[::-1]) if i < j else (j, i, b)
            if pen.length(a[i : j + 1]) < 0.3:
                continue
            ring = a[i : j + 1] + chord[1:-1] + [a[i]]
            tails = [t for t in (a[: i + 1], a[j:]) if len(t) > 1 and pen.length(t) > 0.03]
            return ringed([p for p in paths if p is not a and p is not b] + [ring] + tails)
    return paths


def centerlines(face, ch):
    """A character's strokes, each whole and running as a hand writes it, in no order yet."""
    paths, _ = pen.trace(face, ch)
    paths = [q for p in ringed(pen.join(pen.prune(pen.contract(paths)))) for q in pen.split(spaced(p))]
    paths = [q for p in paths for q in squared(p)]
    paths = [pen.orient(p, [p]) if pen.closed(p) else p for p in paths]
    paths = [q if pen.closed(q) or pen.rank(q[0]) <= pen.rank(q[-1]) else q[::-1] for q in paths]
    return [q for p in paths for q in no_climb(p)]


def spaced(p):
    """A stroke without the points a join left on top of each other."""
    out = [p[0]]
    for q in p[1:]:
        if pen.dist(q, out[-1]) >= 0.004:
            out.append(q)
    return out if len(out) > 1 else p[:2]


def thinned(p):
    """A stroke with only the points its shape needs: a straight one is its two ends."""
    return [[float(x), float(y)] for x, y in approximate_polygon(np.array(p), tolerance=0.006)]


def write(face, ch):
    """A character's strokes in order, and whether its letters were told apart."""
    paths = [thinned(p) for p in centerlines(face, ch)]
    code = ord(ch) - 0xAC00
    if 0 <= code < 11172:
        first, vowel, last = code // 588, code % 588 // 28, code % 28
        parts = syllable(paths, vowel, last)
        if parts and parts[0]:
            return consonant(parts[0], first in DOUBLE_FIRST) + parts[1] + consonant(parts[2], last in DOUBLE_LAST), True
        return by_hand(paths), False
    return by_hand(paths), True


def main():
    licence = ROOT / "assets/licenses/handwritten-xiaolai-OFL.txt"
    assert "SIL OPEN FONT LICENSE" in licence.read_text() and "LXGW" in licence.read_text()
    wanted = [chr(c) for c in range(0xAC00, 0xAC00 + 11172)] + [chr(c) for c in range(0x3131, 0x3164)]
    data, unsure = {}, 0
    for path in sorted((pen.fonts() / "Xiaolai").glob("*.woff2")):
        face, cmap = pen.load(path)
        for ch in wanted:
            if ord(ch) in cmap and ch not in data:
                strokes, sure = write(face, ch)
                assert strokes, f"Empty glyph: {ch}"
                unsure += not sure
                # hanzi-writer's coordinate system, as the kanji are kept, so loading stays shared.
                data[ch] = [[[round(x * 1024), round(900 - y * 1024)] for x, y in s] for s in strokes]
    assert len(data) >= 11220, len(data)
    out = ROOT / "assets/glyphs/hangul.json.gz"
    out.write_bytes(gzip.compress(json.dumps(dict(sorted(data.items())), ensure_ascii=False, separators=(",", ":")).encode(), mtime=0))
    print(f"Generated {len(data)} Korean characters ({unsure} in plain top-to-bottom order); license in {licence.relative_to(ROOT)}.")


if __name__ == "__main__":
    main()
