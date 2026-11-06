//! Layout -> pen strokes, ported from src/ink/compose.ts and src/board/labels.ts.
//! Icons and the pointing highlight are left out of the spike.

use crate::board_layout::{Layout, Rect, Row, FONT, TITLE};
use crate::glyphs::{Glyphs, Pt};
use crate::graph::{Graph, Kind, Node};
use crate::pen::{arrow, circle_mark, line, place, rough_box, strike, Rng};

const WEIGHT: f32 = 0.65;
const ENGLISH_WEIGHT: f32 = 1.0;
const LABEL_PAD: f32 = 3.0;

#[derive(Clone, Copy, PartialEq)]
pub struct Style {
    pub rgb: u32,
    pub alpha: f32,
    pub under: bool,
}

const INK: u32 = 0x1e_1e_1e;
const RED: u32 = 0xe0_31_31;
const BLUE: u32 = 0x19_71_c2;
const HIGHLIGHT: u32 = 0xff_ec_99;
pub const PAPER: u32 = 0xfb_fb_f9;

pub struct Line {
    pub points: Vec<Pt>,
    pub width: f32,
    pub wobble: f32,
    pub style: Style,
}

/// One drawable thing. A key that survives a new layout keeps its ink on the board.
pub struct Piece {
    pub key: String,
    pub seed: String,
    pub lines: Vec<Line>,
}

fn pen_for(size: f32) -> f32 {
    (size * 0.1).max(1.8)
}

fn color_of(n: Option<&Node>, fallback: u32) -> u32 {
    match n.map(|n| n.kind) {
        Some(Kind::Question) => RED,
        Some(Kind::Note) => BLUE,
        _ => fallback,
    }
}

fn alpha(n: Option<&Node>) -> f32 {
    if n.is_some_and(|n| n.marks.iter().any(|m| m == "划掉")) { 0.55 } else { 1.0 }
}

#[derive(Clone, Copy)]
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn area(b: &Rect) -> Area {
    Area { x: b.x as f32, y: b.y as f32, w: b.w as f32, h: b.h as f32 }
}

fn hits_segment(r: Area, a: Pt, b: Pt) -> bool {
    let n = ((b[0] - a[0]).hypot(b[1] - a[1]) / 4.0).ceil().max(1.0) as usize;
    (0..=n).any(|i| {
        let t = i as f32 / n as f32;
        let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
        x > r.x - LABEL_PAD && x < r.x + r.w + LABEL_PAD && y > r.y - LABEL_PAD && y < r.y + r.h + LABEL_PAD
    })
}

fn hits_line(r: Area, points: &[Pt]) -> bool {
    points.windows(2).any(|s| hits_segment(r, s[0], s[1]))
}

fn hits_rect(r: Area, o: Area) -> bool {
    r.x < o.x + o.w && o.x < r.x + r.w && r.y < o.y + o.h && o.y < r.y + r.h
}

/// Along the longest segments first, the first place clear of text, boxes and every line.
fn label_spot(points: &[Pt], label: &str, size: f32, blocks: &[Area], lines: &[Vec<Pt>], prefer: &[Pt]) -> (Area, bool) {
    let w = crate::board_layout::text_width(label, f64::from(size)) as f32;
    let h = size;
    let mut cands: Vec<Pt> = prefer.to_vec();
    let mut segs: Vec<(Pt, Pt)> = points.windows(2).map(|s| (s[0], s[1])).collect();
    segs.sort_by(|s, t| (t.1[0] - t.0[0]).hypot(t.1[1] - t.0[1]).total_cmp(&(s.1[0] - s.0[0]).hypot(s.1[1] - s.0[1])));
    for (a, b) in segs {
        let flat = (b[1] - a[1]).abs() <= (b[0] - a[0]).abs();
        for t in [0.5, 0.3, 0.7] {
            let (mx, my) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            if flat {
                cands.push([mx - w / 2.0, my - h - 8.0]);
                cands.push([mx - w / 2.0, my + 8.0]);
            } else {
                cands.push([mx + 8.0, my - h / 2.0]);
                cands.push([mx - w - 8.0, my - h / 2.0]);
            }
        }
    }
    let free = |p: &&Pt| {
        let r = Area { x: p[0], y: p[1], w, h };
        !blocks.iter().any(|b| hits_rect(r, *b)) && !lines.iter().any(|l| hits_line(r, l))
    };
    let hit = cands.iter().find(free).copied();
    let at = hit.or_else(|| cands.first().copied()).unwrap_or(points[0]);
    (Area { x: at[0], y: at[1], w, h }, hit.is_some())
}

fn edge_point(b: Area, c: Pt, p: Pt) -> Pt {
    let (hw, hh) = (b.w / 2.0 + 12.0, b.h / 2.0 + 12.0);
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    if dx == 0.0 && dy == 0.0 {
        return c;
    }
    let tx = if dx == 0.0 { f32::INFINITY } else { hw / dx.abs() };
    let ty = if dy == 0.0 { f32::INFINITY } else { hh / dy.abs() };
    let t = tx.min(ty).min(1.0);
    [c[0] + dx * t, c[1] + dy * t]
}

struct Canvas<'a> {
    glyphs: &'a mut Glyphs,
    pieces: Vec<Piece>,
}

impl Canvas<'_> {
    /// `size` is the pen size. Text passes per-stroke weights; everything else uses the default.
    fn add(&mut self, key: String, rgb: u32, opacity: f32, under: bool, shapes: Vec<(Vec<Pt>, bool)>, size: f32) {
        let style = Style { rgb, alpha: opacity, under };
        let lines: Vec<Line> = shapes
            .into_iter()
            .map(|(points, latin)| Line { points, width: size * if latin { ENGLISH_WEIGHT } else { WEIGHT }, wobble: size * 0.12, style })
            .collect();
        // The position is in the key: ink that has to move is redrawn in its new place.
        let at = lines.first().and_then(|l| l.points.first()).copied().unwrap_or([0.0, 0.0]);
        let full = format!("{key}@{:.0},{:.0}", at[0], at[1]);
        self.pieces.push(Piece { key: full, seed: key, lines });
    }

    fn write(&mut self, text: &str, x: f32, y: f32, size: f32, seed: &str) -> Vec<(Vec<Pt>, bool)> {
        let mut r = Rng::new(seed);
        let mut out = vec![];
        let mut cx = x;
        for ch in text.chars() {
            let Some(g) = self.glyphs.get(ch) else {
                cx += size * 0.6;
                continue;
            };
            for s in place(&g.strokes, cx, y, size, &mut r) {
                out.push((s, g.latin));
            }
            cx += g.advance * size * (0.96 + r.next() * 0.08) + size * 0.04;
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, n: Option<&Node>, id: &str, s: &str, x: f32, y: f32, size: f32, rgb: u32, w: f32, opacity: Option<f32>) {
        let key = format!("{id}|{s}");
        let opacity = opacity.unwrap_or_else(|| alpha(n));
        if n.is_some_and(|n| n.marks.iter().any(|m| m == "高亮")) {
            let k = format!("{key}|h");
            let mut r = Rng::new(&k);
            let stroke = line([x - 2.0, y + size / 2.0], [x + w + 2.0, y + size / 2.0], &mut r, 0.004);
            self.add(k, HIGHLIGHT, 0.45, true, vec![(stroke, false)], (size + 2.0) / WEIGHT);
        }
        let shapes = self.write(s, x, y, size, &key);
        self.add(key.clone(), rgb, opacity, false, shapes, pen_for(size));
        let Some(n) = n else { return };
        let mut r = Rng::new(&format!("{key}m"));
        let plain = |v: Vec<Vec<Pt>>| v.into_iter().map(|s| (s, false)).collect::<Vec<_>>();
        for mark in &n.marks {
            match mark.as_str() {
                "划掉" => {
                    let shapes = plain(strike(x, y + size * 0.6, w, &mut r));
                    self.add(format!("{key}|x"), INK, 1.0, false, shapes, pen_for(size) * 1.1);
                }
                "下划线" => {
                    let shapes = plain(vec![line([x - 2.0, y + size + 7.0], [x + w + 6.0, y + size + 5.0], &mut r, 0.02)]);
                    self.add(format!("{key}|u"), RED, 1.0, false, shapes, pen_for(size));
                }
                "圈出" => {
                    let shapes = plain(vec![circle_mark(x + w / 2.0, y + size / 2.0, w / 2.0 + 18.0, size / 2.0 + 14.0, &mut r)]);
                    self.add(format!("{key}|o"), RED, 1.0, false, shapes, pen_for(size));
                }
                "星标" => {
                    let shapes = self.write("★", x + w + 10.0, y - 2.0, size, &format!("{key}s"));
                    self.add(format!("{key}|s"), RED, 1.0, false, shapes, pen_for(size));
                }
                _ => {}
            }
        }
    }

    fn row(&mut self, g: &Graph, row: &Row, ox: f32, oy: f32) {
        let n = g.nodes.iter().find(|n| n.id == row.id);
        let (x, y, size, w) = (ox + row.x as f32, oy + row.y as f32, row.size as f32, row.w as f32);
        let rgb = color_of(n, INK);
        let pre = if row.prefix.is_empty() { 0.0 } else { 30.0 };
        if !row.prefix.is_empty() {
            let shapes = self.write(&row.prefix, x, y, size, &format!("{}pre", row.id));
            self.add(format!("{}|pre|{}", row.id, row.prefix), rgb, alpha(n), false, shapes, pen_for(size));
        }
        if n.is_some_and(|n| n.kind == Kind::Formula) {
            let mut r = Rng::new(&format!("{}f", row.id));
            let shape = circle_mark(x + pre + w / 2.0, y + size / 2.0, w / 2.0 + 16.0, size / 2.0 + 10.0, &mut r);
            self.add(format!("{}|f|{}", row.id, row.text), rgb, alpha(n), false, vec![(shape, false)], pen_for(size) * 0.9);
        }
        self.text(n, &row.id, &row.text, x + pre, y, size, rgb, w, None);
    }
}

struct Label {
    key: String,
    text: String,
    route: Vec<Pt>,
    prefer: Vec<Pt>,
    fixed: Option<Area>,
}

/// The strokes for a board, and the edge labels that found no clear spot.
pub fn compose(g: &Graph, l: &Layout, glyphs: &mut Glyphs) -> (Vec<Piece>, Vec<String>) {
    let mut c = Canvas { glyphs, pieces: vec![] };
    let font = FONT as f32;
    let title = TITLE as f32;
    let plain = |v: Vec<Vec<Pt>>| v.into_iter().map(|s| (s, false)).collect::<Vec<_>>();

    for reg in &l.regions {
        let (rx, ry) = (reg.rect.x as f32, reg.rect.y as f32);
        if let Some(t) = &reg.title {
            let head = reg.head.as_ref().and_then(|id| g.nodes.iter().find(|n| &n.id == id));
            let tx = rx + t.x as f32;
            let rgb = if reg.head.is_some() { color_of(head, INK) } else { BLUE };
            let id = reg.head.clone().unwrap_or_else(|| reg.rect.id.clone());
            let tw = t.w as f32;
            c.text(head, &id, &t.text, tx, ry, title, rgb, tw, None);
            let mut r = Rng::new(&format!("{}ul", reg.rect.id));
            let rule = line([tx - 6.0, ry + title + 10.0], [tx + tw + 14.0, ry + title + 13.0], &mut r, 0.03);
            c.add(format!("{}|{}|ul", reg.rect.id, t.text), rgb, alpha(head), false, vec![(rule, false)], pen_for(title));
        }
        for row in &reg.rows {
            c.row(g, row, rx, ry);
        }
    }

    let mut lines: Vec<Vec<Pt>> = vec![];
    let mut labels: Vec<Label> = vec![];
    if let Some((_, d)) = &l.diagram {
        for (b, label) in &d.groups {
            let mut r = Rng::new(&b.id);
            let a = area(b);
            c.add(format!("{}|{label}", b.id), BLUE, 0.8, false, plain(rough_box(a.x, a.y, a.w, a.h, &mut r)), pen_for(font) * 0.8);
            c.text(None, &format!("{}t", b.id), label, a.x + 14.0, a.y + 10.0, font - 2.0, BLUE, 0.0, Some(1.0));
        }
        for b in &d.nodes {
            let Some(n) = g.nodes.iter().find(|n| n.id == b.id) else { continue };
            let a = area(b);
            let mut r = Rng::new(&n.id);
            c.add(format!("{}|box", n.id), color_of(Some(n), INK), alpha(Some(n)), false, plain(rough_box(a.x + 4.0, a.y + 4.0, a.w - 8.0, a.h - 8.0, &mut r)), pen_for(font));
            let tw = n.label.chars().map(|ch| if (ch as u32) < 128 { 0.55 } else { 1.0 }).sum::<f32>() * font;
            c.text(Some(n), &n.id, &n.label, a.x + (a.w - tw) / 2.0, a.y + (a.h - font) / 2.0 - 2.0, font, color_of(Some(n), INK), tw, None);
        }
        for rt in &d.edges {
            let Some(e) = g.edges.iter().find(|e| e.from == rt.from && e.to == rt.to) else { continue };
            let points: Vec<Pt> = rt.points.iter().map(|p| [p[0] as f32, p[1] as f32]).collect();
            let key = format!("{}>{}", e.from, e.to);
            let mut r = Rng::new(&key);
            c.add(key.clone(), INK, 1.0, false, plain(arrow(&points, &mut r)), pen_for(font) * 0.9);
            lines.push(points.clone());
            if let Some(t) = &e.label {
                labels.push(Label { key: format!("{key}l"), text: t.clone(), route: points, prefer: vec![], fixed: None });
            }
        }
    }

    for (from, to, route) in &l.links {
        let Some(e) = g.edges.iter().find(|e| &e.from == from && &e.to == to) else { continue };
        let (Some(a), Some(b)) = (l.anchors.get(from), l.anchors.get(to)) else { continue };
        let key = format!("{from}~{to}");
        let mut r = Rng::new(&key);
        if let Some(rt) = route {
            let points: Vec<Pt> = rt.points.iter().map(|p| [p[0] as f32, p[1] as f32]).collect();
            c.add(key.clone(), INK, 1.0, false, plain(arrow(&points, &mut r)), pen_for(font) * 0.9);
            lines.push(points.clone());
            if let Some(t) = &e.label {
                labels.push(Label { key: format!("{key}l"), text: t.clone(), route: points, prefer: vec![], fixed: rt.label_box.as_ref().map(area) });
            }
            continue;
        }
        let (a, b) = (area(a), area(b));
        let ca = [a.x + a.w / 2.0, a.y + a.h / 2.0];
        let cb = [b.x + b.w / 2.0, b.y + b.h / 2.0];
        let p0 = edge_point(a, ca, cb);
        let p1 = edge_point(b, cb, ca);
        let len = (p1[0] - p0[0]).hypot(p1[1] - p0[1]).max(1.0);
        let bend = (len * 0.15).min(50.0);
        let mid = [(p0[0] + p1[0]) / 2.0 - (p1[1] - p0[1]) / len * bend, (p0[1] + p1[1]) / 2.0 + (p1[0] - p0[0]) / len * bend];
        let curve: Vec<Pt> = (0..=16)
            .map(|i| {
                let t = i as f32 / 16.0;
                let u = 1.0 - t;
                [u * u * p0[0] + 2.0 * u * t * mid[0] + t * t * p1[0], u * u * p0[1] + 2.0 * u * t * mid[1] + t * t * p1[1]]
            })
            .collect();
        c.add(key.clone(), INK, 1.0, false, plain(arrow(&curve, &mut r)), pen_for(font) * 0.9);
        lines.push(curve.clone());
        if let Some(t) = &e.label {
            let on = [(p0[0] + 2.0 * mid[0] + p1[0]) / 4.0, (p0[1] + 2.0 * mid[1] + p1[1]) / 4.0];
            let (nx, ny) = (-(p1[1] - p0[1]) / len, (p1[0] - p0[0]) / len);
            labels.push(Label { key: format!("{key}l"), text: t.clone(), route: curve, prefer: vec![[on[0] + nx * 22.0 - 12.0, on[1] + ny * 22.0 - 10.0]], fixed: None });
        }
    }

    // Labels last, once every line is known, so they can avoid all of them.
    let mut unclear = vec![];
    let mut blocks: Vec<Area> = l.anchors.iter().filter(|(id, _)| !l.regions.iter().any(|r| &r.rect.id == *id)).map(|(_, b)| area(b)).collect();
    for r in &l.regions {
        if let Some(t) = &r.title {
            blocks.push(Area { x: (r.rect.x + t.x) as f32, y: r.rect.y as f32, w: t.w as f32, h: title + 14.0 });
        }
    }
    for lb in labels {
        let size = font - 4.0;
        let (spot, clear) = match lb.fixed {
            Some(a) => (a, false),
            None => label_spot(&lb.route, &lb.text, size, &blocks, &lines, &lb.prefer),
        };
        blocks.push(spot);
        // No clean spot: a paper-coloured backing keeps the text readable over the line.
        if !clear {
            if lb.fixed.is_none() {
                unclear.push(lb.text.clone());
            }
            let mut r = Rng::new(&format!("{}bg", lb.key));
            let cy = spot.y + spot.h / 2.0 + 1.0;
            let backing = line([spot.x - 4.0, cy], [spot.x + spot.w + 4.0, cy], &mut r, 0.0);
            c.add(format!("{}|bg", lb.key), PAPER, 0.92, false, vec![(backing, false)], (spot.h + 8.0) / WEIGHT);
        }
        c.text(None, &lb.key, &lb.text, spot.x, spot.y, size, INK, 0.0, Some(1.0));
    }
    c.pieces.retain(|p| !p.lines.is_empty());
    (c.pieces, unclear)
}
