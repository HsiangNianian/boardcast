//! Layout -> pen strokes, ported from src/ink/compose.ts and src/board/labels.ts.
//! Icons and the pointing highlight are left out of the spike.

use crate::board_layout::{Layout, Rect, Row, FONT, TITLE};
use crate::glyphs::{Glyphs, Pt};
use crate::graph::{Graph, Kind, Node};
use crate::spans::{rotate_point, SpanBox};
use crate::pen::{around, arrow, circle_mark, ink, line, marker, place, rough_box, strike, Inked, Rng};

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
const HIGHLIGHT: u32 = 0xff_e0_66;
const MARKER_ALPHA: f32 = 0.5;
pub const PAPER: u32 = 0xfb_fb_f9;

pub struct Line {
    pub points: Vec<Pt>,
    pub width: f32,
    pub wobble: f32,
    pub style: Style,
    /// Drawn with a highlighter rather than a pen.
    pub flat: bool,
}

impl Line {
    pub fn inked(&self, r: &mut Rng) -> Inked {
        if self.flat {
            marker(&self.points, self.width, self.wobble, r)
        } else {
            ink(&self.points, self.width, self.wobble, r)
        }
    }
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
    // Close to the line first, then further out, as someone writing would look for room.
    for gap in [8.0, 20.0, 34.0] {
        for (a, b) in &segs {
            let flat = (b[1] - a[1]).abs() <= (b[0] - a[0]).abs();
            for t in [0.5, 0.3, 0.7, 0.15, 0.85] {
                let (mx, my) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
                let (above, below) = ([mx - w / 2.0, my - h - gap], [mx - w / 2.0, my + gap]);
                let (right, left) = ([mx + gap, my - h / 2.0], [mx - w - gap, my - h / 2.0]);
                // A slanting line has room off to either side as well.
                cands.extend(if flat { [above, below, right, left] } else { [right, left, above, below] });
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

/// Everything on the board that a connection shouldn't run over, with what it belongs to.
fn text_areas(l: &Layout) -> Vec<(String, Area)> {
    let mut out: Vec<(String, Area)> = l.targets().iter().map(|t| (t.id.clone(), area(t))).collect();
    out.extend(l.regions.iter().filter_map(|r| {
        let (id, title) = (r.head.as_ref()?, r.title.as_ref()?);
        Some((id.clone(), Area { x: (r.rect.x + title.x) as f32, y: r.rect.y as f32, w: title.w as f32, h: TITLE as f32 + 14.0 }))
    }));
    out
}

/// A curved connection, with what its label is placed from.
struct Bow {
    curve: Vec<Pt>,
    mid: Pt,
    p0: Pt,
    p1: Pt,
}

fn bow(p0: Pt, p1: Pt, bend: f32) -> Bow {
    let len = (p1[0] - p0[0]).hypot(p1[1] - p0[1]).max(1.0);
    let amount = bend.signum() * (len * bend.abs()).min(50.0 / 0.15 * bend.abs());
    let mid = [(p0[0] + p1[0]) / 2.0 - (p1[1] - p0[1]) / len * amount, (p0[1] + p1[1]) / 2.0 + (p1[0] - p0[0]) / len * amount];
    let curve = (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            let u = 1.0 - t;
            [u * u * p0[0] + 2.0 * u * t * mid[0] + t * t * p1[0], u * u * p0[1] + 2.0 * u * t * mid[1] + t * t * p1[1]]
        })
        .collect();
    Bow { curve, mid, p0, p1 }
}

/// What a route costs: how much text it runs over, then how much of other lines it lies along.
fn route_cost(curve: &[Pt], from: &str, to: &str, text: &[(String, Area)], lines: &[Vec<Pt>]) -> (usize, usize) {
    let over_text = curve.windows(2).filter(|s| text.iter().any(|(id, a)| id != from && id != to && hits_segment(*a, s[0], s[1]))).count();
    let along = curve.iter().filter(|p| lines.iter().flatten().any(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < 8.0)).count();
    (over_text, along)
}

/// How far each of two opposite connections keeps to its own side, and how little it curves. Bowed
/// from the same two points they would close into a sliver with their arrowheads on top of each other.
const TWO_WAY_SIDE: f32 = 13.0;
const TWO_WAY_BEND: f32 = 0.03;

/// The straight way between two things, moved to its own side: its right as it travels, so that the
/// opposite connection, travelling the other way, takes the other side by itself.
fn to_one_side(p0: Pt, p1: Pt) -> (Pt, Pt) {
    let len = (p1[0] - p0[0]).hypot(p1[1] - p0[1]).max(1.0);
    let (nx, ny) = (-(p1[1] - p0[1]) / len * TWO_WAY_SIDE, (p1[0] - p0[0]) / len * TWO_WAY_SIDE);
    ([p0[0] + nx, p0[1] + ny], [p1[0] + nx, p1[1] + ny])
}

/// A hand draws a connection around what is written: the plain bow if that is clear, which also keeps
/// a board from rearranging itself, or otherwise the clearest of other sides and curves.
fn clear_bow(a: &Rect, b: &Rect, from: &str, to: &str, two_way: bool, text: &[(String, Area)], lines: &[Vec<Pt>]) -> Bow {
    const BENDS: [f32; 6] = [0.15, -0.15, 0.3, -0.3, 0.5, -0.5];
    let (a, b) = (area(a), area(b));
    let (ca, cb) = ([a.x + a.w / 2.0, a.y + a.h / 2.0], [b.x + b.w / 2.0, b.y + b.h / 2.0]);
    let (start, end) = (edge_point(a, ca, cb), edge_point(b, cb, ca));
    let plain = if two_way {
        let (p0, p1) = to_one_side(start, end);
        bow(p0, p1, TWO_WAY_BEND)
    } else {
        bow(start, end, BENDS[0])
    };
    let mut best_cost = route_cost(&plain.curve, from, to, text, lines);
    if best_cost == (0, 0) {
        return plain;
    }
    let sides = |r: Area, c: Pt, toward: Pt| {
        let (dx, dy) = (r.w / 2.0 + 12.0, r.h / 2.0 + 12.0);
        [edge_point(r, c, toward), [c[0] + dx, c[1]], [c[0] - dx, c[1]], [c[0], c[1] - dy], [c[0], c[1] + dy]]
    };
    let mut best = plain;
    for p0 in sides(a, ca, cb) {
        for p1 in sides(b, cb, ca) {
            for bend in BENDS {
                let candidate = bow(p0, p1, bend);
                let cost = route_cost(&candidate.curve, from, to, text, lines);
                if cost < best_cost {
                    (best, best_cost) = (candidate, cost);
                }
            }
        }
    }
    best
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
            .map(|(points, latin)| Line { points, width: size * if latin { ENGLISH_WEIGHT } else { WEIGHT }, wobble: size * 0.12, style, flat: false })
            .collect();
        // The position is in the key: ink that has to move is redrawn in its new place.
        let at = lines.first().and_then(|l| l.points.first()).copied().unwrap_or([0.0, 0.0]);
        let full = format!("{key}@{:.0},{:.0}", at[0], at[1]);
        self.pieces.push(Piece { key: full, seed: key, lines });
    }

    /// One pass of a highlighter along a line of text, as tall as the letters and running a little past them.
    fn marker(&mut self, key: String, x0: f32, x1: f32, y: f32, height: f32, opacity: f32) {
        let mut r = Rng::new(&key);
        let (start, end) = (x0 - 2.0 - r.next() * 3.0, x1 + 2.0 + r.next() * 4.0);
        let drift = (r.next() - 0.5) * height * 0.12;
        let points = line([start, y + height * 0.55 - drift], [end, y + height * 0.55 + drift], &mut r, 0.002);
        let style = Style { rgb: HIGHLIGHT, alpha: MARKER_ALPHA * opacity, under: true };
        let at = points[0];
        let lines = vec![Line { points, width: height * 0.95, wobble: height * 0.012, style, flat: true }];
        self.pieces.push(Piece { key: format!("{key}@{:.0},{:.0}", at[0], at[1]), seed: key, lines });
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
            self.marker(format!("{key}|h"), x, x + w, y, size, 1.0);
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

    fn highlight(&mut self, key: String, boxes: &[[f32; 4]], opacity: f32) {
        for (i, b) in boxes.iter().enumerate() {
            self.marker(format!("{key}|h{i}"), b[0], b[0] + b[2], b[1], b[3], opacity);
        }
    }

    /// Imported text is on the page already, so only what was marked on it is drawn.
    fn page_marks(&mut self, g: &Graph, l: &Layout) {
        let plain = |v: Vec<Vec<Pt>>| v.into_iter().map(|s| (s, false)).collect::<Vec<_>>();
        let font = FONT as f32;
        for n in g.nodes.iter().filter(|n| n.kind == Kind::Region) {
            let Some([x, y, w, h]) = n.at().map(|a| a.map(|v| v as f32)) else { continue };
            let mut r = Rng::new(&format!("{}|pin", n.id));
            let o = alpha(Some(n));
            let has = |mark: &str| n.marks.iter().any(|m| m == mark);
            if has("高亮") {
                self.highlight(n.id.clone(), &[[x, y, w, h]], o);
            }
            if has("圈出") {
                let shape = circle_mark(x + w / 2.0, y + h / 2.0, w / 2.0 + 14.0, h / 2.0 + 12.0, &mut r);
                self.add(format!("{}|o", n.id), RED, o, false, plain(vec![shape]), pen_for(font));
            }
            if has("下划线") {
                let shape = line([x - 2.0, y + h + 6.0], [x + w + 4.0, y + h + 4.0], &mut r, 0.01);
                self.add(format!("{}|u", n.id), RED, o, false, plain(vec![shape]), pen_for(font));
            }
            if has("划掉") {
                let shapes = if n.rows().len() > 1 || h > 40.0 { vec![line([x, y], [x + w, y + h], &mut r, 0.02), line([x + w, y], [x, y + h], &mut r, 0.02)] } else { strike(x, y + h / 2.0, w, &mut r) };
                self.add(format!("{}|x", n.id), RED, o, false, plain(shapes), pen_for(font));
            }
            if has("星标") {
                let shapes = self.write("★", x - font - 10.0, y + (h - font) / 2.0, font, &format!("{}s", n.id));
                self.add(format!("{}|s", n.id), RED, o, false, shapes, pen_for(font));
            }
            for s in n.spans() {
                let places: Vec<String> = s.boxes.iter().map(|b| format!("{},{}", b.x, b.y)).collect();
                let key = format!("{}|{}|{}", n.id, s.mark, places.join(";"));
                let mut r = Rng::new(&key);
                let thin = pen_for(font) * 0.8;
                let turned = |points: Vec<Pt>, angle: Option<f64>| -> Vec<Pt> {
                    points.into_iter().map(|p| {
                        let q = rotate_point([f64::from(p[0]), f64::from(p[1])], angle.unwrap_or(0.0));
                        [q[0] as f32, q[1] as f32]
                    }).collect()
                };
                let area = |b: &SpanBox| [b.x as f32, b.y as f32, b.w as f32, b.h as f32];
                match s.mark.as_str() {
                    "高亮" => {
                        let boxes: Vec<[f32; 4]> = s.boxes.iter().map(area).collect();
                        self.highlight(key, &boxes, o);
                    }
                    "圈出" => {
                        // A new column starts a new loop, so it does not cross text that was not chosen.
                        let mut groups: Vec<Vec<&SpanBox>> = vec![];
                        for b in &s.boxes {
                            match groups.last_mut() {
                                Some(group) if group.last().is_some_and(|last| b.y > last.y && b.angle == last.angle) => group.push(b),
                                _ => groups.push(vec![b]),
                            }
                        }
                        let shapes = groups.iter().map(|boxes| {
                            let b = area(boxes[0]);
                            let shape = if boxes.len() == 1 {
                                circle_mark(b[0] + b[2] / 2.0, b[1] + b[3] / 2.0, b[2] / 2.0 + 6.0, b[3] / 2.0 + 4.0, &mut r)
                            } else {
                                around(&boxes.iter().map(|x| area(x)).collect::<Vec<_>>(), 4.0, &mut r)
                            };
                            turned(shape, boxes[0].angle)
                        }).collect();
                        self.add(key, RED, o, false, plain(shapes), thin);
                    }
                    "下划线" => {
                        let shapes = s.boxes.iter().map(|b| {
                            let a = area(b);
                            turned(line([a[0], a[1] + a[3] + 2.0], [a[0] + a[2], a[1] + a[3] + 2.0], &mut r, 0.004), b.angle)
                        }).collect();
                        self.add(key, RED, o, false, plain(shapes), thin);
                    }
                    "划掉" => {
                        let shapes = s.boxes.iter().flat_map(|b| {
                            let a = area(b);
                            strike(a[0], a[1] + a[3] / 2.0, a[2], &mut r).into_iter().map(|shape| turned(shape, b.angle)).collect::<Vec<_>>()
                        }).collect();
                        self.add(key, RED, o, false, plain(shapes), thin);
                    }
                    "星标" => {
                        let Some(first) = s.boxes.first() else { continue };
                        let shapes = self.write("★", first.x as f32 - 14.0, first.y as f32, 12.0, &key);
                        let shapes = shapes.into_iter().map(|(p, latin)| (turned(p, first.angle), latin)).collect();
                        self.add(key, RED, o, false, shapes, thin);
                    }
                    _ => {}
                }
            }
        }
        // A thin line ties each note in the margin back to the text it is about.
        for row in l.regions.iter().filter(|r| r.rect.id == "__margin").flat_map(|r| &r.rows) {
            let host = g.nodes.iter().find(|n| n.id == row.id).and_then(|n| n.parent.as_ref()).and_then(|p| g.nodes.iter().find(|x| &x.id == p)).and_then(|p| p.at());
            let Some([x, y, w, h]) = host.map(|a| a.map(|v| v as f32)) else { continue };
            let key = format!("{}|tether", row.id);
            let mut r = Rng::new(&key);
            let shape = line([x + w + 6.0, y + h.min(30.0) / 2.0], [row.x as f32 - 8.0, row.y as f32 + row.size as f32 / 2.0], &mut r, 0.02);
            self.add(key, BLUE, 0.6, false, plain(vec![shape]), pen_for(font) * 0.6);
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
#[derive(Clone, Debug, PartialEq)]
pub enum Owner {
    Node(String),
    Edge(String, String),
}

/// A patch of the board some ink lies in, and what that ink belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct InkBox {
    pub owner: Owner,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The words on a connection, rather than the line.
    pub label: bool,
}

fn node_id(text: &str) -> Option<&str> {
    let digits = text.strip_prefix('n')?.chars().take_while(char::is_ascii_digit).count();
    (digits > 0).then(|| &text[..=digits])
}

/// Read what a piece of ink belongs to from its key, like `n1|box`, `n1>n2` or `n1~n2l`.
fn owner_of(seed: &str) -> Option<(Owner, bool)> {
    let from = node_id(seed)?;
    let rest = &seed[from.len()..];
    let Some(after) = rest.strip_prefix('>').or_else(|| rest.strip_prefix('~')) else {
        return Some((Owner::Node(from.to_string()), false));
    };
    let to = node_id(after)?;
    Some((Owner::Edge(from.to_string(), to.to_string()), after[to.len()..].starts_with('l')))
}

/// Where each thing's ink lies. A line is split along its length, as one box around a long diagonal
/// would take in whatever it passes.
pub fn ink_boxes(pieces: &[Piece]) -> Vec<InkBox> {
    const STEP: f32 = 24.0;
    const PAD: f32 = 5.0;
    let mut out = vec![];
    for piece in pieces {
        let Some((owner, label)) = owner_of(&piece.seed) else { continue };
        let mut add = |points: &[Pt]| {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in points {
                (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
            }
            if x0 <= x1 {
                out.push(InkBox { owner: owner.clone(), x: x0 - PAD, y: y0 - PAD, w: x1 - x0 + PAD * 2.0, h: y1 - y0 + PAD * 2.0, label });
            }
        };
        if matches!(owner, Owner::Edge(..)) && !label {
            for line in &piece.lines {
                for pair in line.points.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let parts = ((b[0] - a[0]).hypot(b[1] - a[1]) / STEP).ceil().max(1.0) as usize;
                    let at = |i: usize| {
                        let t = i as f32 / parts as f32;
                        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
                    };
                    for i in 0..parts {
                        add(&[at(i), at(i + 1)]);
                    }
                }
            }
        } else {
            let points: Vec<Pt> = piece.lines.iter().flat_map(|line| line.points.iter().copied()).collect();
            add(&points);
        }
    }
    out
}

pub fn compose(g: &Graph, l: &Layout, glyphs: &mut Glyphs) -> (Vec<Piece>, Vec<String>) {
    let mut c = Canvas { glyphs, pieces: vec![] };
    let font = FONT as f32;
    let title = TITLE as f32;
    let plain = |v: Vec<Vec<Pt>>| v.into_iter().map(|s| (s, false)).collect::<Vec<_>>();

    c.page_marks(g, l);

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

    let text_areas = text_areas(l);
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
        let two_way = l.links.iter().any(|(f, t, r)| f == to && t == from && r.is_none());
        let Bow { curve, mid, p0, p1 } = clear_bow(a, b, from, to, two_way, &text_areas, &lines);
        let len = (p1[0] - p0[0]).hypot(p1[1] - p0[1]).max(1.0);
        c.add(key.clone(), INK, 1.0, false, plain(arrow(&curve, &mut r)), pen_for(font) * 0.9);
        lines.push(curve.clone());
        if let Some(t) = &e.label {
            let on = [(p0[0] + 2.0 * mid[0] + p1[0]) / 4.0, (p0[1] + 2.0 * mid[1] + p1[1]) / 4.0];
            let (nx, ny) = (-(p1[1] - p0[1]) / len, (p1[0] - p0[0]) / len);
            labels.push(Label { key: format!("{key}l"), text: t.clone(), route: curve, prefer: if two_way { vec![] } else { vec![[on[0] + nx * 22.0 - 12.0, on[1] + ny * 22.0 - 10.0]] }, fixed: None });
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
        // Nothing goes under the words, as a hand writes straight on the paper. A label with no clean spot
        // crosses its line, and the check hears about it.
        if !clear && lb.fixed.is_none() {
            unclear.push(lb.text.clone());
        }
        c.text(None, &lb.key, &lb.text, spot.x, spot.y, size, INK, 0.0, Some(1.0));
    }
    c.pieces.retain(|p| !p.lines.is_empty());
    (c.pieces, unclear)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_finds_room_beside_a_slanting_line() {
        let route = [[0.0, 0.0], [300.0, 200.0]];
        let (_, clear) = label_spot(&route, "publishes first", 16.0, &[], &[route.to_vec()], &[]);
        assert!(clear);
    }

    #[test]
    fn ink_is_traced_to_what_it_belongs_to() {
        let piece = |seed: &str, points: Vec<Pt>| Piece { key: format!("{seed}@1,2"), seed: seed.into(), lines: vec![Line { points, width: 1.0, wobble: 0.0, style: Style { rgb: 0, alpha: 1.0, under: false }, flat: false }] };
        let boxes = ink_boxes(&[
            piece("n12|box", vec![[0.0, 0.0], [10.0, 20.0]]),
            piece("n1>n2", vec![[0.0, 0.0], [100.0, 100.0]]),
            piece("n1~n2l", vec![[0.0, 0.0], [30.0, 10.0]]),
            piece("g1", vec![[0.0, 0.0], [1.0, 1.0]]),
        ]);
        assert_eq!(boxes[0], InkBox { owner: Owner::Node("n12".into()), x: -5.0, y: -5.0, w: 20.0, h: 30.0, label: false });
        // A long diagonal is covered in pieces, so it takes in little besides itself.
        let line: Vec<&InkBox> = boxes.iter().filter(|b| b.owner == Owner::Edge("n1".into(), "n2".into()) && !b.label).collect();
        assert!(line.len() >= 5);
        assert!(line.iter().all(|b| b.w < 40.0 && b.h < 40.0));
        let label = boxes.iter().find(|b| b.label).unwrap();
        assert_eq!(label.owner, Owner::Edge("n1".into(), "n2".into()));
        assert!(!boxes.iter().any(|b| matches!(&b.owner, Owner::Node(id) if id.starts_with('g'))), "a group belongs to nobody");
    }

    /// A board as it grows, line by line, since that places things differently from laying it out whole.
    fn grown(ops: &[&str]) -> Layout {
        let (mut g, mut board) = (Graph::default(), crate::board_layout::StableLayout::default());
        let mut l = board.layout(&g, &mut crate::sugiyama::first_diagram);
        for line in ops {
            g.apply(&crate::graph::parse_ops(line));
            l = board.layout(&g, &mut crate::sugiyama::first_diagram);
        }
        l
    }

    /// How many connections run over text on this board: as drawn plainly, and as chosen now.
    fn over_text(l: &Layout) -> (usize, usize) {
        let text = text_areas(l);
        let (mut plain, mut chosen, mut lines) = (0, 0, vec![]);
        for (from, to, route) in &l.links {
            if route.is_some() {
                continue;
            }
            let (Some(a), Some(b)) = (l.anchors.get(from), l.anchors.get(to)) else { continue };
            let (x, y) = (area(a), area(b));
            let (cx, cy) = ([x.x + x.w / 2.0, x.y + x.h / 2.0], [y.x + y.w / 2.0, y.y + y.h / 2.0]);
            let drawn = bow(edge_point(x, cx, cy), edge_point(y, cy, cx), 0.15);
            plain += usize::from(route_cost(&drawn.curve, from, to, &text, &[]).0 > 0);
            let picked = clear_bow(a, b, from, to, false, &text, &lines);
            chosen += usize::from(route_cost(&picked.curve, from, to, &text, &[]).0 > 0);
            lines.push(picked.curve);
        }
        (plain, chosen)
    }

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { id: id.into(), x, y, w, h }
    }

    /// From a report: a topic directly above another, so the plain bow runs through the first one's list.
    #[test]
    fn a_connection_goes_around_its_own_topics_list() {
        let (top, below) = (rect("top", 100.0, 100.0, 200.0, 40.0), rect("below", 100.0, 300.0, 200.0, 40.0));
        let text: Vec<(String, Area)> = [("top", &top), ("below", &below), ("row1", &rect("row1", 100.0, 150.0, 200.0, 30.0)), ("row2", &rect("row2", 100.0, 190.0, 200.0, 30.0))].iter().map(|(id, r)| ((*id).to_string(), area(r))).collect();
        let (x, y) = (area(&top), area(&below));
        let (cx, cy) = ([x.x + x.w / 2.0, x.y + x.h / 2.0], [y.x + y.w / 2.0, y.y + y.h / 2.0]);
        let plain = bow(edge_point(x, cx, cy), edge_point(y, cy, cx), 0.15);
        assert!(route_cost(&plain.curve, "top", "below", &text, &[]).0 > 0, "the case must start out crossing");

        let chosen = clear_bow(&top, &below, "top", "below", false, &text, &[]);
        assert_eq!(route_cost(&chosen.curve, "top", "below", &text, &[]).0, 0);

        // The way back keeps off the first line rather than lying along it.
        let back = clear_bow(&below, &top, "below", "top", false, &text, &[chosen.curve.clone()]);
        assert_eq!(route_cost(&back.curve, "below", "top", &text, &[chosen.curve]), (0, 0));
    }

    /// From a report: two opposite arrows between neighbours closed into an unreadable sliver.
    #[test]
    fn opposite_connections_run_apart_all_the_way() {
        let (left, right) = (rect("left", 0.0, 0.0, 100.0, 40.0), rect("right", 230.0, 0.0, 100.0, 40.0));
        let out = clear_bow(&left, &right, "left", "right", true, &[], &[]);
        let back = clear_bow(&right, &left, "right", "left", true, &[], &[out.curve.clone()]);
        for p in &out.curve {
            let nearest = back.curve.iter().map(|q| (p[0] - q[0]).hypot(p[1] - q[1])).fold(f32::MAX, f32::min);
            assert!(nearest >= 20.0, "{nearest} apart at {p:?}");
        }
    }

    #[test]
    #[test]
    fn a_clear_connection_is_left_as_it_was_so_the_board_stays_put() {
        let (left, right) = (rect("left", 0.0, 0.0, 100.0, 40.0), rect("right", 400.0, 0.0, 100.0, 40.0));
        let (x, y) = (area(&left), area(&right));
        let (cx, cy) = ([x.x + x.w / 2.0, x.y + x.h / 2.0], [y.x + y.w / 2.0, y.y + y.h / 2.0]);
        assert_eq!(clear_bow(&left, &right, "left", "right", false, &[], &[]).curve, bow(edge_point(x, cx, cy), edge_point(y, cy, cx), 0.15).curve);
    }

    #[test]
    fn recorded_boards_have_no_more_lines_over_text_than_before() {
        let goldens: serde_json::Value = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        let (mut boards, mut plain, mut chosen) = (0, 0, 0);
        let mut count = |ops: &[String]| {
            let l = grown(&ops.iter().map(String::as_str).collect::<Vec<_>>());
            let (p, c) = over_text(&l);
            (plain, chosen, boards) = (plain + p, chosen + c, boards + 1);
        };
        let (mut file, mut ops) = (String::new(), vec![]);
        for step in goldens["steps"].as_array().unwrap() {
            if step["file"].as_str() != Some(file.as_str()) {
                count(&ops);
                ops.clear();
                file = step["file"].as_str().unwrap().to_string();
            }
            ops.push(step["ops"].as_str().unwrap().to_string());
        }
        count(&ops);
        for case in goldens["stress"].as_array().unwrap() {
            count(&case["ops"].as_str().unwrap().lines().map(str::to_string).collect::<Vec<_>>());
        }
        println!("{boards} boards: {plain} connections over text before, {chosen} now");
        assert!(chosen <= plain);
    }
}
