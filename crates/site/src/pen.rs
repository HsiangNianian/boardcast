//! A pen that writes in stroke order. The letters come from EMS Tech, a single-stroke font: each glyph is a
//! few centerlines, so the pen can follow them the way a hand would rather than revealing filled letters.

use crate::util::{frame, length_of, now, reduced, set_attr, svg_el, to_d, Pt, Rand};
use std::collections::HashMap;
use std::sync::OnceLock;
use web_sys::Element;

struct Font {
    em: f64,
    glyphs: HashMap<char, (String, f64)>,
}

fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        let raw: serde_json::Value =
            serde_json::from_str(include_str!("emstech.json")).expect("the pen's font");
        let glyphs = raw["g"]
            .as_object()
            .expect("glyphs")
            .iter()
            .filter_map(|(ch, g)| {
                Some((
                    ch.chars().next()?,
                    (g[0].as_str()?.to_string(), g[1].as_f64()?),
                ))
            })
            .collect();
        Font {
            em: raw["em"].as_f64().expect("em"),
            glyphs,
        }
    })
}

pub struct Word {
    pub text: String,
    pub x: f64,
    pub w: f64,
}

pub struct Line {
    pub strokes: Vec<Vec<Pt>>,
    pub words: Vec<Word>,
    pub width: f64,
}

/// A line of text as pen strokes in page units, with where each word sits.
pub fn text_strokes(s: &str, x: f64, y: f64, size: f64, seed: u32) -> Line {
    let font = font();
    let k = size / font.em;
    let mut r = Rand::new(seed);
    let (mut strokes, mut words) = (vec![], vec![]);
    let (mut cx, mut word_start, mut word) = (x, x, String::new());
    let space = &font.glyphs[&' '];
    for ch in s.chars().chain([' ']) {
        if ch == ' ' && !word.is_empty() {
            words.push(Word {
                text: std::mem::take(&mut word),
                x: word_start,
                w: cx - word_start,
            });
        }
        let g = font.glyphs.get(&ch).unwrap_or(space);
        if ch != ' ' {
            if word.is_empty() {
                word_start = cx;
            }
            word.push(ch);
        }
        // The glyphs only use M and L: every M starts a new stroke.
        for part in g.0.split('M').filter(|t| !t.trim().is_empty()) {
            let n: Vec<f64> = part
                .replace('L', " ")
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            let mut pts = vec![];
            for pair in n.chunks_exact(2) {
                let jx = (r.next() - 0.5) * size * 0.012;
                let jy = (r.next() - 0.5) * size * 0.012;
                pts.push((cx + pair[0] * k + jx, y - pair[1] * k + jy));
            }
            if pts.len() > 1 {
                strokes.push(pts);
            }
        }
        cx += g.1 * k * 0.97;
    }
    Line {
        strokes,
        words,
        width: cx - x,
    }
}

pub struct Pen<'a> {
    pub class: &'a str,
    /// Page units per millisecond.
    pub speed: f64,
    /// The pause between strokes, as a distance the pen travels in the air.
    pub lift: f64,
}

impl Default for Pen<'_> {
    fn default() -> Self {
        Self {
            class: "w",
            speed: 1.6,
            lift: 40.0,
        }
    }
}

/// Draw strokes one after another at the pen's speed, with a visible tip. Returns the group drawn.
pub async fn pen_draw(svg: &Element, strokes: &[Vec<Pt>], pen: Pen<'_>) -> Element {
    let g = svg_el("g", &[], Some(svg));
    let paths: Vec<(Element, &Vec<Pt>, f64)> = strokes
        .iter()
        .map(|pts| {
            (
                svg_el("path", &[("class", pen.class.into())], Some(&g)),
                pts,
                length_of(pts),
            )
        })
        .collect();
    if reduced() {
        for (p, pts, _) in &paths {
            set_attr(p, "d", to_d(pts));
        }
        return g;
    }
    let tip = svg_el("g", &[], Some(svg));
    svg_el(
        "circle",
        &[("class", "tip".into()), ("r", "6".into())],
        Some(&tip),
    );
    svg_el(
        "circle",
        &[("class", "tip-dot".into()), ("r", "1.6".into())],
        Some(&tip),
    );
    let total: f64 = paths.iter().map(|(_, _, len)| len + pen.lift).sum();
    let mut full = vec![false; paths.len()];
    let t0 = now();
    loop {
        let t = frame().await;
        let mut dist = (t - t0) * pen.speed;
        let mut at = None;
        for (i, (p, pts, len)) in paths.iter().enumerate() {
            if dist >= len + pen.lift {
                if !full[i] {
                    set_attr(p, "d", to_d(pts));
                    full[i] = true;
                }
                dist -= len + pen.lift;
                continue;
            }
            if dist <= 0.0 {
                break;
            }
            // Partway through this stroke: draw up to the pen.
            let mut out = vec![pts[0]];
            let mut left = dist.min(*len);
            for w in pts.windows(2) {
                if left <= 0.0 {
                    break;
                }
                let (a, b) = (w[0], w[1]);
                let seg = (b.0 - a.0).hypot(b.1 - a.1);
                if left >= seg {
                    out.push(b);
                    left -= seg;
                } else {
                    let k = left / seg;
                    out.push((a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k));
                    left = 0.0;
                }
            }
            set_attr(p, "d", to_d(&out));
            at = out.last().copied();
            break;
        }
        if let Some((x, y)) = at {
            set_attr(&tip, "transform", format!("translate({x} {y})"));
        }
        if (t - t0) * pen.speed >= total {
            break;
        }
    }
    tip.remove();
    for (p, pts, _) in &paths {
        set_attr(p, "d", to_d(pts));
    }
    g
}
