//! The marker pen: where a hand puts each character (`place`), and the shape of the ink it leaves along
//! a line (`ink`, `outline`), pressing harder or lighter along the stroke as a hand does (`pressure`).

use crate::glyphs::{Pt, Stroke};

/// mulberry32 seeded from a string, so a character keeps its shape across layouts.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: &str) -> Self {
        let mut h: u32 = 2_166_136_261;
        for c in seed.encode_utf16() {
            h = (h ^ u32::from(c)).wrapping_mul(16_777_619);
        }
        Self(h)
    }

    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64 / 4_294_967_296.0) as f32
    }
}

/// Points every `step` along the line, ending on its last point, so the pen's edge is worked out from
/// evenly spaced points however the line was given.
fn resample(s: &[Pt], step: f32) -> Stroke {
    if s.len() < 2 {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len() * 2);
    out.push(s[0]);
    let mut since = 0.0;
    for w in s.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (b[0] - a[0]).hypot(b[1] - a[1]);
        if d <= 0.0 {
            continue;
        }
        let mut t = step - since;
        while t <= d {
            out.push([a[0] + (b[0] - a[0]) * t / d, a[1] + (b[1] - a[1]) * t / d]);
            t += step;
        }
        since = d - (t - step);
    }
    let last = s[s.len() - 1];
    if since > step * 0.3 || out.len() < 2 {
        out.push(last);
    } else if let Some(end) = out.last_mut() {
        *end = last;
    }
    out
}

/// Each bend of a line turned through a short curve about `r` long, as a pen tip goes round it rather than
/// stopping dead; the stroke data is polylines.
fn round_bends(points: &[Pt], r: f32) -> Stroke {
    if points.len() < 3 {
        return points.to_vec();
    }
    let toward = |p: Pt, q: Pt, k: f32| {
        let d = (q[0] - p[0]).hypot(q[1] - p[1]).max(0.0001);
        [p[0] + (q[0] - p[0]) * k / d, p[1] + (q[1] - p[1]) * k / d]
    };
    let mut out = Vec::with_capacity(points.len() * 4);
    out.push(points[0]);
    for w in points.windows(3) {
        let (a, p, b) = (w[0], w[1], w[2]);
        let k = r.min((p[0] - a[0]).hypot(p[1] - a[1]) / 3.0).min((b[0] - p[0]).hypot(b[1] - p[1]) / 3.0);
        let (s, e) = (toward(p, a, k), toward(p, b, k));
        for t in [0.0, 0.25, 0.5, 0.75, 1.0_f32] {
            let (u, v, w) = ((1.0 - t) * (1.0 - t), 2.0 * (1.0 - t) * t, t * t);
            out.push([u * s[0] + v * p[0] + w * e[0], u * s[1] + v * p[1] + w * e[1]]);
        }
    }
    out.push(points[points.len() - 1]);
    out
}

fn wobble(s: &mut [Pt], amp: f32, r: &mut Rng) {
    let f = 0.5 + r.next() * 2.0;
    let phase = r.next() * std::f32::consts::TAU;
    let last = (s.len().max(2) - 1) as f32;
    for (i, p) in s.iter_mut().enumerate() {
        let d = (i as f32 / last * std::f32::consts::PI * f + phase).sin() * amp;
        p[0] += d * 0.6;
        p[1] += d;
    }
}

/// Chaikin's corner cutting: each pass rounds every corner a little more, so a messy hand swings
/// through a turn instead of writing it.
fn chaikin(s: Stroke, times: usize) -> Stroke {
    let mut out = s;
    for _ in 0..times {
        if out.len() <= 2 {
            break;
        }
        let mut next = Vec::with_capacity(out.len() * 2);
        next.push(out[0]);
        for w in out.windows(2) {
            let (a, b) = (w[0], w[1]);
            next.push([a[0] * 0.75 + b[0] * 0.25, a[1] * 0.75 + b[1] * 0.25]);
            next.push([a[0] * 0.25 + b[0] * 0.75, a[1] * 0.25 + b[1] * 0.75]);
        }
        next.push(out[out.len() - 1]);
        out = next;
    }
    out
}

/// Both ends of a stroke run a little past where they should, or stop a little short.
fn overshoot(mut s: Stroke, amount: f32, r: &mut Rng) -> Stroke {
    if s.len() < 2 || amount <= 0.0 {
        return s;
    }
    let past = |a: Pt, b: Pt, k: f32| [a[0] + (a[0] - b[0]) * k, a[1] + (a[1] - b[1]) * k];
    let n = s.len();
    s[0] = past(s[0], s[1], (r.next() - 0.4) * amount);
    s[n - 1] = past(s[n - 1], s[n - 2], (r.next() - 0.3) * amount);
    s
}

/// Place a character at (x, y), as a hand writes it. `messy` runs from 0 (neat) to 1 (very messy) and
/// moves everything a hurried hand does: the character's tilt, size, baseline and lean to the right; each
/// stroke's own shift and turn; how far corners are rounded; and ends that run past or stop short.
/// Characters are in font units (size 1, (0.5, 0.5) the middle). At 0 the strokes keep their shape.
pub fn place(strokes: &[Stroke], x: f32, y: f32, s: f32, r: &mut Rng, messy: f32) -> Vec<Stroke> {
    let m = messy.clamp(0.0, 1.0);
    let rot = (r.next() - 0.5) * (0.05 + 0.16 * m);
    let k = s * (1.0 + (r.next() - 0.5) * (0.08 + 0.22 * m));
    let dy = (r.next() - 0.5) * s * (0.04 + 0.12 * m);
    let slant = 0.04 + 0.2 * m;
    let (sin, cos) = rot.sin_cos();
    // Strokes packed close (播) are shaken less, so a dense character keeps its parts apart.
    let room = (6.0 / strokes.len().max(1) as f32).min(1.0);
    strokes
        .iter()
        .map(|stroke| {
            let (sx, sy, sr) = ((r.next() - 0.5) * 0.06 * m * room, (r.next() - 0.5) * 0.06 * m * room, (r.next() - 0.5) * 0.25 * m * room);
            let local: Stroke = if m > 0.0 && !stroke.is_empty() {
                let n = stroke.len() as f32;
                let (cx, cy) = (stroke.iter().map(|p| p[0]).sum::<f32>() / n, stroke.iter().map(|p| p[1]).sum::<f32>() / n);
                let (ss, sc) = sr.sin_cos();
                stroke.iter().map(|[u, v]| [cx + (u - cx) * sc - (v - cy) * ss + sx, cy + (u - cx) * ss + (v - cy) * sc + sy]).collect()
            } else {
                stroke.clone()
            };
            overshoot(chaikin(local, (m * 1.4).round() as usize), 0.2 * m, r)
                .iter()
                .map(|[u, v]| {
                    let px = (u - 0.5 + (0.5 - v) * slant) * k;
                    let py = (v - 0.5) * k;
                    [x + s * 0.5 + px * cos - py * sin, y + s * 0.5 + dy + px * sin + py * cos]
                })
                .collect()
        })
        .collect()
}

/// Skeleton after densify and wobble. `outline` turns any prefix of it into a filled shape.
#[derive(Clone)]
pub struct Inked {
    pub center: Stroke,
    pub width: f32,
    pub length: f32,
    /// A highlighter: a flat tip of one width from end to end, rather than a pen that responds to speed.
    pub flat: bool,
}

pub fn ink(points: &[Pt], width: f32, wobble_amp: f32, r: &mut Rng) -> Inked {
    let mut center = resample(&round_bends(points, width * 0.6), width * 0.3);
    wobble(&mut center, wobble_amp, r);
    let length = center.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
    Inked { center, width, length, flat: false }
}

/// A pass of a highlighter along these points.
pub fn marker(points: &[Pt], width: f32, wobble_amp: f32, r: &mut Rng) -> Inked {
    Inked { flat: true, ..ink(points, width, wobble_amp, r) }
}

/// How the pen leaves the paper, judged from where the stroke is heading at its end (y runs down).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lift {
    /// Flicked off upwards, as a hook or a rising stroke: a short, sharp tail.
    Flick,
    /// Swept off down and to the left: a long tail.
    Sweep,
    /// Pressed into the paper going down and to the right, then lifted: it swells, then tails off.
    Swell,
    /// Stopped with a small press: across, or straight down.
    Stop,
}

/// `end` is where the stroke heads over its last part, `whole` from its start to its end, both as unit
/// vectors with y down.
fn lift(end: (f32, f32), whole: (f32, f32)) -> Lift {
    if end.0 < -0.3 && end.1 > 0.2 {
        Lift::Sweep
    } else if whole.0 > 0.35 && whole.1 > 0.35 && end.0 > 0.5 {
        // A fall to the right, even one that flattens out at its foot (人).
        Lift::Swell
    } else {
        Lift::Stop
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The point about `d` along the line from `pts[i]`, one way or the other.
fn reach(pts: &[Pt], at: &[f32], i: usize, d: f32, back: bool) -> Pt {
    let mut j = i;
    if back {
        while j > 0 && at[i] - at[j] < d {
            j -= 1;
        }
    } else {
        while j + 1 < pts.len() && at[j] - at[i] < d {
            j += 1;
        }
    }
    pts[j]
}

/// How hard the pen presses along a whole stroke, as a share of its width, one value per point.
/// It lands with a small press, runs a little lighter along a long stretch, bears down in a corner, and
/// leaves the paper as the stroke's last direction has it (`Lift`). A short stroke is a dot: it lands
/// light and ends full, a drop of ink.
fn pressure(pts: &[Pt], width: f32) -> Vec<f32> {
    let mut at = Vec::with_capacity(pts.len());
    let mut walked = 0.0;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            walked += (p[0] - pts[i - 1][0]).hypot(p[1] - pts[i - 1][1]);
        }
        at.push(walked);
    }
    let total = walked.max(0.001);
    if total < width * 3.0 {
        return at.iter().map(|a| 0.62 + 0.5 * smooth(a / total)).collect();
    }
    let closed = (pts[0][0] - pts[pts.len() - 1][0]).hypot(pts[0][1] - pts[pts.len() - 1][1]) < width * 1.5;
    // How the pen leaves: a hook or a rising tick by its very end; otherwise by where the stroke's last part
    // heads, and the whole of it.
    let heading = |back: f32| {
        let from = reach(pts, &at, pts.len() - 1, back, true);
        let end = pts[pts.len() - 1];
        let (dx, dy) = (end[0] - from[0], end[1] - from[1]);
        let n = dx.hypot(dy).max(0.001);
        (dx / n, dy / n)
    };
    let lift = match (closed, heading((width * 1.5).min(total * 0.3))) {
        (true, _) => Lift::Stop,
        (false, (_, dy)) if dy < -0.35 => Lift::Flick,
        _ => lift(heading(total * 0.4), heading(total)),
    };
    let raw: Vec<f32> = (0..pts.len())
        .map(|i| {
            let a = at[i];
            let left = total - a;
            let mut p = 0.9;
            // Landing.
            p += 0.1 * (-((a - width * 0.5) / (width * 0.9)).powi(2)).exp();
            // A long stretch runs lighter.
            p -= 0.08 * smooth((a.min(left) - width * 2.0) / (width * 6.0));
            // A corner: the hand slows and presses.
            let (b, c) = (reach(pts, &at, i, width * 1.2, true), reach(pts, &at, i, width * 1.2, false));
            let (u, v) = ([pts[i][0] - b[0], pts[i][1] - b[1]], [c[0] - pts[i][0], c[1] - pts[i][1]]);
            let (lu, lv) = (u[0].hypot(u[1]), v[0].hypot(v[1]));
            if lu > width * 0.3 && lv > width * 0.3 {
                let turn = ((u[0] * v[0] + u[1] * v[1]) / (lu * lv)).clamp(-1.0, 1.0).acos();
                p += 0.22 * smooth((turn - 0.8) / 1.0);
            }
            // Leaving the paper.
            match lift {
                Lift::Flick => p *= 1.0 - 0.78 * smooth(1.0 - left / (width * 1.8).min(total * 0.4)),
                Lift::Sweep => p *= 1.0 - 0.75 * smooth(1.0 - left / (width * 3.0).min(total * 0.45)),
                Lift::Swell => {
                    let span = (width * 3.0).min(total * 0.45);
                    let u = 1.0 - left / span;
                    if u > 0.0 {
                        p *= if u < 0.65 { 1.0 + 0.3 * (u / 0.65 * std::f32::consts::FRAC_PI_2).sin() } else { 1.3 - 1.0 * smooth((u - 0.65) / 0.35) };
                    }
                }
                Lift::Stop => {
                    if !closed {
                        p += 0.07 * (-((left - width * 0.4) / (width * 0.7)).powi(2)).exp();
                    }
                }
            }
            p.clamp(0.15, 1.3)
        })
        .collect();
    // Evened out over a few points, so the line swells and thins rather than going lumpy; the ends keep
    // their own value, a tail stays sharp.
    (0..raw.len())
        .map(|i| {
            if i < 2 || i + 2 >= raw.len() {
                return raw[i];
            }
            raw[i - 2..=i + 2].iter().sum::<f32>() / 5.0
        })
        .collect()
}

/// Half the line's width at full pressure, as a share of the pen's size.
const RADIUS: f32 = 0.55;
const CAP_STEPS: usize = 8;

/// The closed outline of a stroke drawn up to `upto` of its length (0..=1). The pressure belongs to the
/// whole stroke, so a stroke being written has the same weight as it will have, only shorter.
pub fn outline(s: &Inked, upto: f32) -> Vec<Pt> {
    let target = s.length * upto;
    let mut pts: Vec<Pt> = Vec::with_capacity(s.center.len());
    let mut press: Vec<f32> = Vec::with_capacity(s.center.len());
    let whole = if s.flat || s.center.len() < 2 { vec![] } else { pressure(&s.center, s.width) };
    let mut walked = 0.0;
    for (i, p) in s.center.iter().enumerate() {
        if i > 0 {
            let q = s.center[i - 1];
            let d = (p[0] - q[0]).hypot(p[1] - q[1]);
            if walked + d >= target {
                let t = if d > 0.0 { (target - walked) / d } else { 0.0 };
                pts.push([q[0] + (p[0] - q[0]) * t, q[1] + (p[1] - q[1]) * t]);
                if !whole.is_empty() {
                    press.push(whole[i - 1] + (whole[i] - whole[i - 1]) * t);
                }
                break;
            }
            walked += d;
        }
        pts.push(*p);
        if let Some(w) = whole.get(i) {
            press.push(*w);
        }
    }
    if pts.len() < 2 {
        let c = pts.first().copied().unwrap_or([0.0, 0.0]);
        return circle(c, s.width * 0.5);
    }

    if s.flat {
        return flat_outline(&pts, s.width * 0.5);
    }

    let mut left = Vec::with_capacity(pts.len() + CAP_STEPS);
    let mut right = Vec::with_capacity(pts.len() + CAP_STEPS);
    let mut heading = 0.0_f32;
    let mut radius = s.width * RADIUS;
    for i in 0..pts.len() {
        let p = pts[i];
        // The edge follows the line's direction over a few points, so a bend in the line does not notch it.
        let a = pts[i.saturating_sub(2)];
        let b = pts[(i + 2).min(pts.len() - 1)];
        radius = s.width * RADIUS * press[i];
        heading = (b[1] - a[1]).atan2(b[0] - a[0]);
        let (ny, nx) = heading.sin_cos();
        left.push([p[0] + ny * radius, p[1] - nx * radius]);
        right.push([p[0] - ny * radius, p[1] + nx * radius]);
    }

    let start_heading = (pts[1][1] - pts[0][1]).atan2(pts[1][0] - pts[0][0]);
    let mut out = Vec::with_capacity(left.len() * 2 + CAP_STEPS * 2);
    out.extend_from_slice(&left);
    cap(&mut out, pts[pts.len() - 1], radius, heading);
    out.extend(right.iter().rev());
    cap(&mut out, pts[0], s.width * RADIUS * press[0], start_heading + std::f32::consts::PI);
    out
}

/// The band a flat tip leaves: both edges waver a little on their own, and the tip is held at a slant, so
/// the ends are cut at an angle rather than rounded.
fn flat_outline(pts: &[Pt], radius: f32) -> Vec<Pt> {
    let slant = radius * 0.45;
    let mut left = Vec::with_capacity(pts.len());
    let mut right = Vec::with_capacity(pts.len());
    let mut along = 0.0;
    for (i, p) in pts.iter().enumerate() {
        let a = pts[i.saturating_sub(1)];
        let b = pts[(i + 1).min(pts.len() - 1)];
        along += (p[0] - a[0]).hypot(p[1] - a[1]);
        let (ny, nx) = (b[1] - a[1]).atan2(b[0] - a[0]).sin_cos();
        let up = radius * (1.0 + 0.05 * (along * 0.031).sin() + 0.03 * (along * 0.083 + 1.0).sin());
        let down = radius * (1.0 + 0.05 * (along * 0.023 + 2.0).sin() + 0.03 * (along * 0.071).sin());
        left.push([p[0] + ny * up + nx * slant, p[1] - nx * up + ny * slant]);
        right.push([p[0] - ny * down - nx * slant, p[1] + nx * down - ny * slant]);
    }
    left.extend(right.iter().rev());
    left
}

/// A half circle around `c`, swept from the left of `heading` to its right.
fn cap(out: &mut Vec<Pt>, c: Pt, radius: f32, heading: f32) {
    for i in 1..CAP_STEPS {
        let a = heading - std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / CAP_STEPS as f32;
        out.push([c[0] + a.cos() * radius, c[1] + a.sin() * radius]);
    }
}

fn circle(c: Pt, radius: f32) -> Vec<Pt> {
    (0..8)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 8.0;
            [c[0] + a.cos() * radius, c[1] + a.sin() * radius]
        })
        .collect()
}

// Marks and shapes: approximate hand-drawn paths given to the same pen, as in pen.ts.

pub fn line(a: Pt, b: Pt, r: &mut Rng, sag: f32) -> Stroke {
    let len = (b[0] - a[0]).hypot(b[1] - a[1]).max(0.001);
    let (nx, ny) = (-(b[1] - a[1]) / len, (b[0] - a[0]) / len);
    let bow = (r.next() - 0.5) * 2.0 * sag * len;
    vec![a, [(a[0] + b[0]) / 2.0 + nx * bow, (a[1] + b[1]) / 2.0 + ny * bow], b]
}

/// A circle a little over one turn, with start and end apart like a quick hand loop.
pub fn circle_mark(cx: f32, cy: f32, rx: f32, ry: f32, r: &mut Rng) -> Stroke {
    let start = r.next() * std::f32::consts::TAU;
    let turns = 1.08 + r.next() * 0.12;
    (0..=40)
        .map(|i| {
            let k = i as f32 / 40.0;
            let t = start + k * turns * std::f32::consts::TAU;
            let grow = 1.0 + k * 0.06;
            [cx + t.cos() * rx * grow, cy + t.sin() * ry * grow]
        })
        .collect()
}

pub fn strike(x: f32, y: f32, w: f32, r: &mut Rng) -> Vec<Stroke> {
    let first = line([x - 4.0, y + 2.0], [x + w + 4.0, y - 3.0], r, 0.03);
    if r.next() > 0.5 {
        vec![first, line([x, y + 6.0], [x + w, y + 2.0], r, 0.03)]
    } else {
        vec![first]
    }
}

pub fn rough_box(x: f32, y: f32, w: f32, h: f32, r: &mut Rng) -> Vec<Stroke> {
    fn o(r: &mut Rng) -> f32 {
        (r.next() - 0.3) * 6.0
    }
    let (a, b, c) = (o(r), o(r), o(r));
    let top = line([x + a, y], [x + w + b, y + c * 0.3], r, 0.01);
    let (a, b, c) = (o(r), o(r), o(r));
    let right = line([x + w, y + a], [x + w + b * 0.3, y + h + c], r, 0.01);
    let (a, b, c) = (o(r), o(r), o(r));
    let bottom = line([x + w + a, y + h], [x + b, y + h + c * 0.3], r, 0.01);
    let (a, b, c) = (o(r), o(r), o(r));
    let left = line([x, y + h + a], [x + b * 0.3, y + c], r, 0.01);
    vec![top, right, bottom, left]
}

/// One stroke for the shaft along `points`, two for the head.
/// A line around several boxes one above another: a sentence over more than one line of text.
pub fn around(boxes: &[[f32; 4]], pad: f32, r: &mut Rng) -> Stroke {
    let (mut right, mut left): (Vec<Pt>, Vec<Pt>) = (vec![], vec![]);
    for b in boxes {
        right.push([b[0] + b[2] + pad, b[1] - pad / 2.0]);
        right.push([b[0] + b[2] + pad, b[1] + b[3] + pad / 2.0]);
        left.insert(0, [b[0] - pad, b[1] - pad / 2.0]);
        left.insert(0, [b[0] - pad, b[1] + b[3] + pad / 2.0]);
    }
    let mut corners = right;
    corners.extend(left);
    corners.push(corners[0]);
    corners.windows(2).flat_map(|w| line(w[0], w[1], r, 0.008)).collect()
}

pub fn arrow(points: &[Pt], r: &mut Rng) -> Vec<Stroke> {
    let shaft = if points.len() == 2 { line(points[0], points[1], r, 0.04) } else { points.to_vec() };
    let end = shaft[shaft.len() - 1];
    let prev = shaft[shaft.len().saturating_sub(2)];
    let angle = (end[1] - prev[1]).atan2(end[0] - prev[0]);
    let spread = 0.45 + r.next() * 0.15;
    let head = |s: f32| vec![[end[0] - (angle + s).cos() * 14.0, end[1] - (angle + s).sin() * 14.0], end];
    vec![shaft.clone(), head(spread), head(-spread)]
}

/// A five-pointed star the way a hand draws one: a single line from the top, crossing itself to each point in
/// turn and back up. The points sit a little unevenly and the whole star leans a touch.
pub fn star(cx: f32, cy: f32, radius: f32, r: &mut Rng) -> Stroke {
    let lean = (r.next() - 0.5) * 0.16;
    (0..=5)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + lean + std::f32::consts::TAU * 2.0 * i as f32 / 5.0;
            let k = radius * (0.92 + r.next() * 0.14);
            [cx + a.cos() * k, cy + a.sin() * k]
        })
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    fn widths(points: &[Pt], width: f32) -> Vec<f32> {
        let center = resample(points, width * 0.3);
        pressure(&center, width)
    }

    #[test]
    fn the_pen_leaves_the_paper_as_the_stroke_heads() {
        // A sweep down to the left (撇) tails off; a line across stops with a small press.
        let sweep = widths(&[[100.0, 0.0], [60.0, 60.0], [0.0, 100.0]], 10.0);
        assert!(sweep[sweep.len() - 1] < 0.4 && sweep[0] > 0.9, "{sweep:?}");
        let across = widths(&[[0.0, 0.0], [150.0, 0.0]], 10.0);
        assert!(across[across.len() - 1] > 0.9);
        // A hook flicks off upwards sharply.
        let hook = widths(&[[0.0, 0.0], [0.0, 120.0], [-15.0, 105.0]], 10.0);
        assert!(hook[hook.len() - 1] < 0.4, "{hook:?}");
        // A long line runs lighter in its middle than where it lands.
        assert!(across[across.len() / 2] < across[2]);
    }

    #[test]
    fn a_dot_lands_light_and_ends_full() {
        let dot = widths(&[[0.0, 0.0], [12.0, 12.0]], 10.0);
        assert!(dot[0] < 0.7 && dot[dot.len() - 1] > 1.0, "{dot:?}");
    }

    #[test]
    fn a_stroke_being_written_is_as_wide_as_it_will_be() {
        let mut r = Rng::new("s");
        let s = ink(&[[0.0, 0.0], [80.0, 10.0], [120.0, 90.0]], 6.0, 0.5, &mut r);
        let (half, whole) = (outline(&s, 0.5), outline(&s, 1.0));
        // The left edge of the half-written stroke follows the finished one.
        let n = half.len() / 4;
        for (a, b) in half[..n].iter().zip(&whole[..n]) {
            assert!((a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01);
        }
    }

    #[test]
    fn a_neat_hand_keeps_each_stroke_as_it_is_and_a_messy_one_does_not() {
        let strokes = vec![vec![[0.1, 0.2], [0.9, 0.2]], vec![[0.5, 0.1], [0.5, 0.9]]];
        let neat = place(&strokes, 0.0, 0.0, 100.0, &mut Rng::new("十"), 0.0);
        let again = place(&strokes, 0.0, 0.0, 100.0, &mut Rng::new("十"), 0.0);
        assert_eq!(neat, again);
        // Neat: each stroke keeps its points, only the character moves as a whole.
        assert_eq!(neat[0].len(), 2);
        let messy = place(&strokes, 0.0, 0.0, 100.0, &mut Rng::new("十"), 1.0);
        assert_ne!(neat, messy);
    }
}
