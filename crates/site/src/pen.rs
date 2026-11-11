//! A pen that writes in stroke order. The letters are the centerlines the app writes English with, traced from
//! Excalifont by `scripts/excalifont-strokes.py`, so the pen follows them the way a hand would rather than
//! revealing filled letters.

use crate::util::{frame, length_of, now, reduced, set_attr, svg_el, to_d, Pt};
use std::collections::HashMap;
use std::sync::OnceLock;
use web_sys::Element;

/// Where the baseline sits in the glyphs' own units, one unit being the font size.
const BASELINE: f64 = 0.88;

#[derive(serde::Deserialize)]
struct Glyph {
    strokes: Vec<Vec<Pt>>,
    advance: f64,
}

fn font() -> &'static HashMap<char, Glyph> {
    static FONT: OnceLock<HashMap<char, Glyph>> = OnceLock::new();
    FONT.get_or_init(|| serde_json::from_str(include_str!("excalifont.json")).expect("the pen's font"))
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

/// A line of text as pen strokes in page units, with where each word sits. `y` is the baseline.
pub fn text_strokes(s: &str, x: f64, y: f64, size: f64) -> Line {
    let font = font();
    let (mut strokes, mut words) = (vec![], vec![]);
    let (mut cx, mut word_start, mut word) = (x, x, String::new());
    let space = &font[&' '];
    for ch in s.chars().chain([' ']) {
        if ch == ' ' && !word.is_empty() {
            words.push(Word { text: std::mem::take(&mut word), x: word_start, w: cx - word_start });
        }
        let g = font.get(&ch).unwrap_or(space);
        if ch != ' ' {
            if word.is_empty() {
                word_start = cx;
            }
            word.push(ch);
        }
        for stroke in &g.strokes {
            if stroke.len() > 1 {
                strokes.push(stroke.iter().map(|&(px, py)| (cx + px * size, y + (py - BASELINE) * size)).collect());
            }
        }
        cx += g.advance * size;
    }
    Line { strokes, words, width: cx - x }
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
        Self { class: "w", speed: 1.6, lift: 40.0 }
    }
}

/// Draw strokes one after another at the pen's speed, with a visible tip. Returns the group drawn.
pub async fn pen_draw(svg: &Element, strokes: &[Vec<Pt>], pen: Pen<'_>) -> Element {
    let g = svg_el("g", &[], Some(svg));
    let paths: Vec<(Element, &Vec<Pt>, f64)> = strokes.iter().map(|pts| (svg_el("path", &[("class", pen.class.into())], Some(&g)), pts, length_of(pts))).collect();
    if reduced() {
        for (p, pts, _) in &paths {
            set_attr(p, "d", to_d(pts));
        }
        return g;
    }
    let tip = svg_el("g", &[], Some(svg));
    svg_el("circle", &[("class", "tip".into()), ("r", "6".into())], Some(&tip));
    svg_el("circle", &[("class", "tip-dot".into()), ("r", "1.6".into())], Some(&tip));
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
