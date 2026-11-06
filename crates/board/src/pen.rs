//! The marker pen, ported from src/ink/pen.ts with the app's defaults (messy = 0).
//! The outline follows perfect-freehand's pressure model but is not a line-for-line port,
//! so strokes look alike rather than match the current app pixel for pixel.

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

fn densify(s: &[Pt], step: f32) -> Stroke {
    let mut out = Vec::with_capacity(s.len() * 2);
    for (i, a) in s.iter().enumerate() {
        out.push(*a);
        let Some(b) = s.get(i + 1) else { break };
        let n = ((b[0] - a[0]).hypot(b[1] - a[1]) / step).floor() as usize;
        for k in 1..n {
            let t = k as f32 / n as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
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

/// Place a character at (x, y). With messy = 0 this leaves only the small tilt, scale and baseline
/// drift; the per-stroke draws are still consumed so the random sequence stays aligned with pen.ts.
pub fn place(strokes: &[Stroke], x: f32, y: f32, s: f32, r: &mut Rng) -> Vec<Stroke> {
    let rot = (r.next() - 0.5) * 0.05;
    let k = s * (1.0 + (r.next() - 0.5) * 0.08);
    let dy = (r.next() - 0.5) * s * 0.04;
    let slant = 0.04;
    let (sin, cos) = rot.sin_cos();
    strokes
        .iter()
        .map(|stroke| {
            for _ in 0..3 {
                r.next();
            }
            stroke
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
pub struct Inked {
    pub center: Stroke,
    pub width: f32,
    pub length: f32,
}

pub fn ink(points: &[Pt], width: f32, wobble_amp: f32, r: &mut Rng) -> Inked {
    let mut center = densify(points, width * 0.3);
    wobble(&mut center, wobble_amp, r);
    let length = center.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
    Inked { center, width, length }
}

const THINNING: f32 = 0.32;
const PRESSURE_RATE: f32 = 0.275;
const CAP_STEPS: usize = 5;

/// The closed outline of a stroke drawn up to `upto` of its length (0..=1).
pub fn outline(s: &Inked, upto: f32) -> Vec<Pt> {
    let target = s.length * upto;
    let mut pts: Vec<Pt> = Vec::with_capacity(s.center.len());
    let mut walked = 0.0;
    for (i, p) in s.center.iter().enumerate() {
        if i > 0 {
            let q = s.center[i - 1];
            let d = (p[0] - q[0]).hypot(p[1] - q[1]);
            if walked + d >= target {
                let t = if d > 0.0 { (target - walked) / d } else { 0.0 };
                pts.push([q[0] + (p[0] - q[0]) * t, q[1] + (p[1] - q[1]) * t]);
                break;
            }
            walked += d;
        }
        pts.push(*p);
    }
    if pts.len() < 2 {
        let c = pts.first().copied().unwrap_or([0.0, 0.0]);
        return circle(c, s.width * 0.5);
    }

    let taper = if s.length > s.width * 5.0 { (s.width * 2.0).min(s.length * 0.2) } else { 0.0 };
    let mut pressure = 0.25_f32;
    let mut left = Vec::with_capacity(pts.len() + CAP_STEPS);
    let mut right = Vec::with_capacity(pts.len() + CAP_STEPS);
    let mut along = 0.0;
    let mut radius = s.width * 0.5;
    let mut heading = 0.0_f32;
    for i in 0..pts.len() {
        let p = pts[i];
        let a = pts[i.saturating_sub(1)];
        let b = pts[(i + 1).min(pts.len() - 1)];
        let step = if i > 0 { (p[0] - a[0]).hypot(p[1] - a[1]) } else { 0.0 };
        along += step;
        // Faster movement (a longer step) lowers the simulated pressure, which thins the line.
        let speed = (step / s.width).min(1.0);
        pressure += (1.0 - speed - pressure) * (speed * PRESSURE_RATE);
        radius = s.width * (0.5 - THINNING * (0.5 - pressure)).max(0.05);
        // Only a finished stroke tapers; a stroke being written keeps a blunt tip.
        if taper > 0.0 && upto >= 1.0 && s.length - along < taper {
            radius *= ((s.length - along) / taper).max(0.1);
        }
        heading = (b[1] - a[1]).atan2(b[0] - a[0]);
        let (ny, nx) = heading.sin_cos();
        left.push([p[0] + ny * radius, p[1] - nx * radius]);
        right.push([p[0] - ny * radius, p[1] + nx * radius]);
    }

    let start_heading = (pts[1][1] - pts[0][1]).atan2(pts[1][0] - pts[0][0]);
    let start_radius = s.width * 0.5 * (0.5 - THINNING * 0.25) * 2.0;
    let mut out = Vec::with_capacity(left.len() * 2 + CAP_STEPS * 2);
    out.extend_from_slice(&left);
    cap(&mut out, pts[pts.len() - 1], radius, heading);
    out.extend(right.iter().rev());
    cap(&mut out, pts[0], start_radius.min(s.width * 0.5), start_heading + std::f32::consts::PI);
    out
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

/// A five-pointed star in one continuous stroke.
pub fn star(cx: f32, cy: f32, radius: f32) -> Stroke {
    (0..=10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 5.0;
            let k = if i % 2 == 0 { radius } else { radius * 0.45 };
            [cx + a.cos() * k, cy + a.sin() * k]
        })
        .collect()
}
