//! Board -> coordinates, ported from src/board/layout.ts without imported pages. Deterministic, and
//! incremental: once placed, regions and flow nodes stay put as a meeting grows.
//! Only the first flow layout differs, as ELK is replaced by the injected `first` function.

use crate::graph::{Graph, Kind, Node};
use std::collections::HashMap;

pub const FONT: f64 = 20.0;
pub const TITLE: f64 = 28.0;
const SMALL: f64 = 17.0;
pub const NODE_H: f64 = 64.0;
const GAP: f64 = 90.0;
const NODE_GAP: f64 = 40.0;
const DIAGRAM: &str = "__diagram";
const LOOSE: &str = "__loose";

pub type P = [f64; 2];

#[derive(Clone, Debug, Default)]
pub struct Rect {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub from: String,
    pub to: String,
    pub points: Vec<P>,
    pub label_box: Option<Rect>,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub prefix: String,
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub size: f64,
}

#[derive(Clone, Debug)]
pub struct Title {
    pub text: String,
    pub x: f64,
    pub w: f64,
}

#[derive(Clone, Debug)]
pub struct Region {
    pub rect: Rect,
    pub head: Option<String>,
    pub title: Option<Title>,
    pub rows: Vec<Row>,
    pub links: Vec<Route>,
}

#[derive(Clone, Debug, Default)]
pub struct Diagram {
    pub x0: f64,
    pub y0: f64,
    pub w: f64,
    pub h: f64,
    pub nodes: Vec<Rect>,
    pub groups: Vec<(Rect, String)>,
    pub edges: Vec<Route>,
}

#[derive(Clone, Debug, Default)]
pub struct Bounds {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub regions: Vec<Region>,
    pub diagram: Option<(Rect, Diagram)>,
    /// Edges outside the flow diagram; without points they are drawn as a curve between anchors.
    pub links: Vec<(String, String, Option<Route>)>,
    pub anchors: HashMap<String, Rect>,
    pub bounds: Bounds,
}

impl Layout {
    /// What can be clicked: flow boxes, list rows and topic titles, but not a topic's whole column.
    pub fn targets(&self) -> Vec<Rect> {
        let mut out: Vec<Rect> = vec![];
        if let Some((_, d)) = &self.diagram {
            out.extend(d.nodes.iter().cloned());
        }
        for r in &self.regions {
            if let (Some(head), Some(t)) = (&r.head, &r.title) {
                out.push(Rect { id: head.clone(), x: r.rect.x + t.x, y: r.rect.y, w: t.w, h: TITLE + 10.0 });
            }
            for row in &r.rows {
                let pre = if row.prefix.is_empty() { 0.0 } else { 30.0 };
                out.push(Rect { id: row.id.clone(), x: r.rect.x + row.x, y: r.rect.y + row.y, w: row.w + pre, h: row.size + 8.0 });
            }
        }
        out
    }
}

/// The target under a board position, if any.
pub fn target_at(targets: &[Rect], x: f64, y: f64) -> Option<&Rect> {
    targets.iter().find(|b| x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h)
}

/// Rough width: CJK characters take a full em, others a bit over half.
pub fn text_width(s: &str, size: f64) -> f64 {
    s.chars()
        .map(|c| if matches!(c as u32, 0x2E80..=0x9FFF | 0xFF00..=0xFFEF) { size } else { size * 0.62 })
        .sum()
}

/// A stable offset, so the same id never jitters between layouts.
fn jitter(id: &str, amp: f64) -> f64 {
    let mut h: u32 = 2_166_136_261;
    for c in id.chars() {
        let unit = c.encode_utf16(&mut [0; 2])[0];
        h = (h ^ u32::from(unit)).wrapping_mul(16_777_619);
    }
    (f64::from(h) / f64::from(u32::MAX) - 0.5) * 2.0 * amp
}

fn is_attach(n: &Node) -> bool {
    matches!(n.kind, Kind::Question | Kind::Formula | Kind::Note)
}

fn has_parent(g: &Graph, n: &Node) -> bool {
    n.parent.as_deref().is_some_and(|p| g.nodes.iter().any(|o| o.id == p))
}

fn is_head(g: &Graph, n: &Node) -> bool {
    if has_parent(g, n) {
        return false;
    }
    n.kind == Kind::Topic || (n.kind == Kind::Node && g.children(&n.id).any(|c| c.kind == Kind::Item))
}

fn is_diagram(g: &Graph, n: &Node) -> bool {
    n.kind == Kind::Node && !is_head(g, n) && !has_parent(g, n)
}

fn prefix_for(n: &Node, depth: usize, index: usize) -> String {
    match n.kind {
        Kind::Question => "？".into(),
        Kind::Note => "↳".into(),
        Kind::Formula => String::new(),
        _ if depth == 1 => format!("{}.", index + 1),
        _ if depth == 2 => format!("{}.", char::from(b'a' + (index % 26) as u8)),
        _ => "·".into(),
    }
}

fn topic_region(g: &Graph, head: &Node) -> Region {
    let mut rows = vec![];
    let mut y = TITLE + 26.0;
    fn walk(g: &Graph, parent: &str, depth: usize, y: &mut f64, rows: &mut Vec<Row>) {
        let mut index = 0;
        // Items first, then attachments.
        let mut kids: Vec<&Node> = g.children(parent).collect();
        kids.sort_by_key(|n| is_attach(n));
        for c in kids {
            let d = if is_attach(c) { depth.max(1) } else { depth };
            let size = if is_attach(c) { SMALL } else { FONT };
            let prefix = prefix_for(c, d, index);
            if !is_attach(c) {
                index += 1;
            }
            let x = 8.0 + (d as f64 - 1.0) * 30.0;
            rows.push(Row { id: c.id.clone(), prefix, text: c.label.clone(), x, y: *y, w: text_width(&c.label, size), size });
            *y += if c.kind == Kind::Formula { size + 30.0 } else { size + 16.0 };
            walk(g, &c.id, depth + 1, y, rows);
        }
    }
    walk(g, &head.id, 1, &mut y, &mut rows);
    let title_w = text_width(&head.label, TITLE) + if head.icon.is_some() { 46.0 } else { 0.0 };
    let row_w = |r: &Row| r.x + if r.prefix.is_empty() { 0.0 } else { 30.0 } + r.w + 24.0;
    let mut w = rows.iter().map(row_w).fold(title_w, f64::max) + 16.0;
    let mut boxes: HashMap<&str, Rect> = HashMap::new();
    boxes.insert(&head.id, Rect { id: head.id.clone(), x: 0.0, y: 0.0, w: title_w, h: TITLE });
    for r in &rows {
        let pre = if r.prefix.is_empty() { 0.0 } else { 30.0 };
        boxes.insert(&r.id, Rect { id: r.id.clone(), x: r.x, y: r.y, w: r.w + pre, h: r.size + 8.0 });
    }
    // A topic's region contains its children, so explicit edges take a lane outside the text.
    let mut links = vec![];
    for e in g.edges.iter().filter(|e| e.from != e.to) {
        let (Some(a), Some(b)) = (boxes.get(e.from.as_str()), boxes.get(e.to.as_str())) else { continue };
        let label_w = e.label.as_deref().map_or(0.0, |l| text_width(l, FONT - 4.0));
        let lane = w + 24.0 + label_w / 2.0;
        let (ay, by) = (a.y + a.h / 2.0, b.y + b.h / 2.0);
        let label_box = e.label.as_ref().filter(|l| !l.is_empty()).map(|_| Rect {
            id: format!("{}>{}:label", e.from, e.to),
            x: lane - label_w / 2.0,
            y: (ay + by) / 2.0 - (FONT - 4.0) / 2.0,
            w: label_w,
            h: FONT - 4.0,
        });
        links.push(Route {
            from: e.from.clone(),
            to: e.to.clone(),
            points: vec![[a.x + a.w + 14.0, ay], [lane, ay], [lane, by], [b.x + b.w + 14.0, by]],
            label_box,
        });
        w = lane + label_w / 2.0 + 24.0;
    }
    let title = Title { text: head.label.clone(), x: if head.icon.is_some() { 46.0 } else { 0.0 }, w: text_width(&head.label, TITLE) };
    Region { rect: Rect { id: head.id.clone(), x: 0.0, y: 0.0, w, h: y + 8.0 }, head: Some(head.id.clone()), title: Some(title), rows, links }
}

fn loose_region(g: &Graph, loose: &[&Node]) -> Region {
    let en = crate::graph::is_english(g.nodes.iter().map(|n| n.label.as_str()));
    let questions = loose.iter().any(|n| n.kind == Kind::Question);
    let notes = loose.iter().any(|n| n.kind != Kind::Question);
    let names = if en { ["Notes & questions", "Open", "Notes"] } else { ["补充与问题", "待定", "补充"] };
    let heading = names[if questions { usize::from(!notes) } else { 2 }];
    let mut rows = vec![];
    let mut seen = vec![];
    let mut y = TITLE + 26.0;
    fn walk(g: &Graph, n: &Node, depth: usize, en: bool, y: &mut f64, rows: &mut Vec<Row>, seen: &mut Vec<String>) {
        if seen.contains(&n.id) {
            return;
        }
        seen.push(n.id.clone());
        let host = if depth == 0 { n.parent.as_deref().and_then(|p| g.nodes.iter().find(|o| o.id == p)) } else { None };
        let text = host.map_or_else(|| n.label.clone(), |h| format!("{}{}{}", h.label, if en { ": " } else { "：" }, n.label));
        rows.push(Row { id: n.id.clone(), prefix: prefix_for(n, depth + 1, 0), w: text_width(&text, FONT), text, x: 8.0 + depth as f64 * 30.0, y: *y, size: FONT });
        *y += if n.kind == Kind::Formula { FONT + 30.0 } else { FONT + 16.0 };
        for c in g.children(&n.id) {
            walk(g, c, depth + 1, en, y, rows, seen);
        }
    }
    for n in loose {
        walk(g, n, 0, en, &mut y, &mut rows, &mut seen);
    }
    let title_w = text_width(heading, TITLE);
    let w = rows.iter().map(|r| r.x + 30.0 + r.w + 24.0).fold(title_w, f64::max) + 16.0;
    Region {
        rect: Rect { id: LOOSE.into(), x: 0.0, y: 0.0, w, h: y + 8.0 },
        head: None,
        title: Some(Title { text: heading.into(), x: 0.0, w: title_w }),
        rows,
        links: vec![],
    }
}

pub fn node_w(n: &Node) -> f64 {
    (text_width(&n.label, FONT) + 44.0 + if n.icon.is_some() { 40.0 } else { 0.0 }).max(110.0)
}

pub fn overlaps(a: &Rect, b: &Rect, m: f64) -> bool {
    a.x < b.x + b.w + m && b.x < a.x + a.w + m && a.y < b.y + b.h + m && b.y < a.y + a.h + m
}

/// Whether a horizontal or vertical run of the polyline passes through another node.
pub fn crosses(points: &[P], boxes: &[Rect]) -> bool {
    points.windows(2).any(|s| {
        let seg = Rect { id: String::new(), x: s[0][0].min(s[1][0]), y: s[0][1].min(s[1][1]), w: (s[1][0] - s[0][0]).abs(), h: (s[1][1] - s[0][1]).abs() };
        boxes.iter().any(|b| overlaps(&seg, &Rect { id: String::new(), x: b.x + 4.0, y: b.y + 4.0, w: b.w - 8.0, h: b.h - 8.0 }, 0.0))
    })
}

/// A polyline for an incrementally placed edge, detouring around nodes in the way.
pub fn route(a: &Rect, b: &Rect, others: &[Rect]) -> Vec<P> {
    let (ay, by) = (a.y + a.h / 2.0, b.y + b.h / 2.0);
    let (ax, bx) = (a.x + a.w / 2.0, b.x + b.w / 2.0);
    let mut cands: Vec<Vec<P>> = vec![];
    if b.x >= a.x + a.w + 20.0 {
        let sx = a.x + a.w;
        let mx = (sx + b.x) / 2.0;
        cands.push(if (ay - by).abs() < 2.0 { vec![[sx, ay], [b.x, by]] } else { vec![[sx, ay], [mx, ay], [mx, by], [b.x, by]] });
    } else if b.y >= a.y + a.h + 20.0 || a.y >= b.y + b.h + 20.0 {
        let (top, bottom) = if b.y >= a.y { (a.y + a.h, b.y) } else { (a.y, b.y + b.h) };
        let mid = (top + bottom) / 2.0;
        cands.push(vec![[ax, top], [ax, mid], [bx, mid], [bx, bottom]]);
    }
    let x0 = a.x.min(b.x);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let between: Vec<&Rect> = others.iter().filter(|o| o.x < x1 && o.x + o.w > x0).collect();
    let low = between.iter().map(|o| o.y + o.h).fold((a.y + a.h).max(b.y + b.h), f64::max) + 28.0;
    let high = between.iter().map(|o| o.y).fold(a.y.min(b.y), f64::min) - 28.0;
    cands.push(vec![[ax, a.y + a.h], [ax, low], [bx, low], [bx, b.y + b.h]]);
    cands.push(vec![[ax, a.y], [ax, high], [bx, high], [bx, b.y]]);
    let fallback = cands.len() - 2;
    cands.iter().position(|c| !crosses(c, others)).map_or_else(|| cands[fallback].clone(), |i| cands[i].clone())
}

fn signature(from: &str, to: &str, at: &[Rect]) -> String {
    let find = |id: &str| at.iter().find(|b| b.id == id).expect("edge end placed");
    let (a, b) = (find(from), find(to));
    [a.x, a.y, a.w, b.x, b.y, b.w].map(|v| format!("{v:.1}")).join(",")
}

/// The layout for one board, remembering positions from one call to the next.
#[derive(Default)]
pub struct StableLayout {
    region_at: HashMap<String, (f64, f64)>,
    node_at: Vec<Rect>,
    route_at: HashMap<String, (String, Vec<P>)>,
    box_at: HashMap<String, Rect>,
}

struct Slot {
    key: String,
    at: usize,
    region: Option<Region>,
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
}

impl StableLayout {
    fn diagram(&mut self, g: &Graph, ids: &[String], first: &mut dyn FnMut(&Graph, &[String]) -> Diagram) -> Diagram {
        self.node_at.retain(|b| ids.contains(&b.id));
        let inner: Vec<_> = g.edges.iter().filter(|e| ids.contains(&e.from) && ids.contains(&e.to)).collect();
        if self.node_at.is_empty() {
            let d = first(g, ids);
            self.node_at.clone_from(&d.nodes);
            self.route_at.clear();
            for e in &d.edges {
                self.route_at.insert(format!("{}>{}", e.from, e.to), (signature(&e.from, &e.to, &self.node_at), e.points.clone()));
            }
            return d;
        }

        // Old nodes stay; a new one goes next to what it connects to, avoiding other regions.
        let (ox, oy) = self.region_at.get(DIAGRAM).copied().unwrap_or((0.0, 0.0));
        let others: Vec<Rect> = self
            .box_at
            .iter()
            .filter(|(k, _)| k.as_str() != DIAGRAM)
            .map(|(_, b)| Rect { id: String::new(), x: b.x - ox - GAP / 2.0, y: b.y - oy - GAP / 2.0, w: b.w + GAP, h: b.h + GAP })
            .collect();
        let mut placed: Vec<Rect> = vec![];
        for n in &g.nodes {
            if let Some(old) = self.node_at.iter().find(|b| b.id == n.id).filter(|_| ids.contains(&n.id)) {
                placed.push(Rect { w: node_w(n), ..old.clone() });
            }
        }
        for n in &g.nodes {
            if !ids.contains(&n.id) || placed.iter().any(|b| b.id == n.id) {
                continue;
            }
            let w = node_w(n);
            let find = |id: &str, placed: &[Rect]| placed.iter().find(|b| b.id == id).cloned();
            let from = inner.iter().find(|e| e.to == n.id && find(&e.from, &placed).is_some());
            let to = inner.iter().find(|e| e.from == n.id && find(&e.to, &placed).is_some());
            let mate = n.group.as_ref().and_then(|grp| g.nodes.iter().find(|m| m.group.as_ref() == Some(grp) && find(&m.id, &placed).is_some()));
            let mut bx = Rect { id: n.id.clone(), x: 0.0, y: 0.0, w, h: NODE_H };
            if let Some(e) = from {
                let p = find(&e.from, &placed).expect("found");
                (bx.x, bx.y) = (p.x + p.w + 80.0, p.y);
            } else if let Some(e) = to {
                let q = find(&e.to, &placed).expect("found");
                (bx.x, bx.y) = (q.x - w - 80.0, q.y);
            } else if let Some(m) = mate {
                let m = find(&m.id, &placed).expect("found");
                (bx.x, bx.y) = (m.x, m.y + m.h + NODE_GAP);
            } else if !placed.is_empty() {
                bx.x = placed.iter().map(|b| b.x).fold(f64::MAX, f64::min);
                bx.y = placed.iter().map(|b| b.y + b.h).fold(f64::MIN, f64::max) + NODE_GAP;
            }
            // Search alternately up and down, keeping the hull clear of other regions if possible.
            let step = NODE_H + NODE_GAP;
            let free = |b: &Rect, strict: bool| {
                if placed.iter().any(|p| overlaps(b, p, 24.0)) {
                    return false;
                }
                if !strict || placed.is_empty() {
                    return true;
                }
                let x = placed.iter().map(|p| p.x).fold(b.x, f64::min);
                let y = placed.iter().map(|p| p.y).fold(b.y, f64::min);
                let right = placed.iter().map(|p| p.x + p.w).fold(b.x + b.w, f64::max);
                let bottom = placed.iter().map(|p| p.y + p.h).fold(b.y + b.h, f64::max);
                let hull = Rect { id: String::new(), x, y, w: right - x, h: bottom - y };
                !others.iter().any(|r| overlaps(&hull, r, 0.0))
            };
            let mut tries = vec![0_i32];
            for i in 1..=30 {
                tries.push(-i);
                tries.push(i);
            }
            let y0 = bx.y;
            let at = |k: i32| Rect { y: y0 + f64::from(k) * step, ..bx.clone() };
            let hit = tries.iter().find(|k| free(&at(**k), true)).or_else(|| tries.iter().find(|k| free(&at(**k), false)));
            bx.y = y0 + f64::from(hit.copied().unwrap_or(0)) * step;
            placed.push(bx);
        }
        self.node_at.clone_from(&placed);

        let mut groups = vec![];
        for grp in &g.groups {
            let members: Vec<&Rect> = placed.iter().filter(|b| g.nodes.iter().any(|n| n.id == b.id && n.group.as_ref() == Some(&grp.id))).collect();
            if members.is_empty() {
                continue;
            }
            let x = members.iter().map(|b| b.x).fold(f64::MAX, f64::min) - 24.0;
            let y = members.iter().map(|b| b.y).fold(f64::MAX, f64::min) - 44.0;
            let x1 = members.iter().map(|b| b.x + b.w).fold(f64::MIN, f64::max) + 24.0;
            let y1 = members.iter().map(|b| b.y + b.h).fold(f64::MIN, f64::max) + 24.0;
            groups.push((Rect { id: grp.id.clone(), x, y, w: x1 - x, h: y1 - y }, grp.label.clone()));
        }

        let mut edges = vec![];
        for e in &inner {
            let key = format!("{}>{}", e.from, e.to);
            let sig = signature(&e.from, &e.to, &placed);
            let rest: Vec<Rect> = placed.iter().filter(|b| b.id != e.from && b.id != e.to).cloned().collect();
            let keep = self.route_at.get(&key).filter(|(s, points)| *s == sig && !crosses(points, &rest)).map(|(_, p)| p.clone());
            let points = keep.unwrap_or_else(|| {
                let find = |id: &str| placed.iter().find(|b| b.id == id).expect("edge end placed");
                route(find(&e.from), find(&e.to), &rest)
            });
            self.route_at.insert(key, (sig, points.clone()));
            edges.push(Route { from: e.from.clone(), to: e.to.clone(), points, label_box: None });
        }

        let boxes = placed.iter().chain(groups.iter().map(|(b, _)| b));
        let pts = edges.iter().flat_map(|e| e.points.iter());
        let mut x0 = f64::MAX;
        let mut y0 = f64::MAX;
        let mut x1 = f64::MIN;
        let mut y1 = f64::MIN;
        for b in boxes {
            x0 = x0.min(b.x);
            y0 = y0.min(b.y);
            x1 = x1.max(b.x + b.w);
            y1 = y1.max(b.y + b.h);
        }
        for p in pts {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        Diagram { x0, y0, w: x1 - x0, h: y1 - y0, nodes: placed, groups, edges }
    }

    /// A new region goes beside or below an existing one, keeping the board close to 16:10.
    fn free_spot(key: &str, w: f64, h: f64, placed: &[Rect]) -> Rect {
        let jx = jitter(key, 14.0);
        let jy = jitter(&format!("{key}y"), 10.0);
        let rect = |x, y| Rect { id: key.to_string(), x, y, w, h };
        if placed.is_empty() {
            return rect(jx, jy);
        }
        let x0 = placed.iter().map(|b| b.x).fold(f64::MAX, f64::min);
        let y0 = placed.iter().map(|b| b.y).fold(f64::MAX, f64::min);
        let x1 = placed.iter().map(|b| b.x + b.w).fold(f64::MIN, f64::max);
        let y1 = placed.iter().map(|b| b.y + b.h).fold(f64::MIN, f64::max);
        let mut best = None;
        let mut best_cost = f64::INFINITY;
        for p in placed {
            for c in [rect(p.x + p.w + GAP + jx, p.y + jy), rect(p.x + jx, p.y + p.h + GAP + jy)] {
                if placed.iter().any(|b| overlaps(&c, b, GAP / 2.0 - 1.0)) {
                    continue;
                }
                let bw = x1.max(c.x + c.w) - x0.min(c.x);
                let bh = y1.max(c.y + c.h) - y0.min(c.y);
                let cost = (bw / 1.6).max(bh) + c.y * 0.01 + c.x * 0.001;
                if cost < best_cost {
                    best_cost = cost;
                    best = Some(c);
                }
            }
        }
        best.unwrap_or_else(|| rect(x0 + jx, y1 + GAP + jy))
    }

    pub fn layout(&mut self, g: &Graph, first: &mut dyn FnMut(&Graph, &[String]) -> Diagram) -> Layout {
        let order = |id: &str| g.nodes.iter().position(|n| n.id == id).expect("node exists");
        let ids: Vec<String> = g.nodes.iter().filter(|n| is_diagram(g, n)).map(|n| n.id.clone()).collect();
        let loose: Vec<&Node> = g
            .nodes
            .iter()
            .filter(|n| is_attach(n) && (!has_parent(g, n) || n.parent.as_ref().is_some_and(|p| ids.contains(p))))
            .collect();
        let mut slots = vec![];
        for n in &g.nodes {
            if is_head(g, n) {
                let r = topic_region(g, n);
                slots.push(Slot { key: r.rect.id.clone(), at: order(&n.id), x0: 0.0, y0: 0.0, w: r.rect.w, h: r.rect.h, region: Some(r) });
            }
        }
        let mut dia = None;
        if ids.is_empty() {
            self.node_at.clear();
        } else {
            let d = self.diagram(g, &ids, first);
            let at = ids.iter().map(|id| order(id)).min().expect("not empty");
            slots.push(Slot { key: DIAGRAM.into(), at, region: None, x0: d.x0, y0: d.y0, w: d.w, h: d.h });
            dia = Some(d);
        }
        if !loose.is_empty() {
            let r = loose_region(g, &loose);
            let at = loose.iter().map(|n| order(&n.id)).min().expect("not empty");
            slots.push(Slot { key: LOOSE.into(), at, x0: 0.0, y0: 0.0, w: r.rect.w, h: r.rect.h, region: Some(r) });
        }
        slots.sort_by_key(|s| s.at);
        self.region_at.retain(|k, _| slots.iter().any(|s| &s.key == k));
        self.box_at.retain(|k, _| slots.iter().any(|s| &s.key == k));

        // Remembered regions return to their place, pushed down if an earlier one has grown over them.
        let mut placed: Vec<Rect> = vec![];
        let mut origin: HashMap<String, (f64, f64)> = HashMap::new();
        for s in &slots {
            let mut bx = if let Some(&(x, y)) = self.region_at.get(&s.key) {
                let mut b = Rect { id: s.key.clone(), x: x + s.x0, y: y + s.y0, w: s.w, h: s.h };
                for _ in 0..60 {
                    let hit: Vec<&Rect> = placed.iter().filter(|o| overlaps(&b, o, GAP / 2.0 - 1.0)).collect();
                    if hit.is_empty() {
                        break;
                    }
                    b.y = hit.iter().map(|o| o.y + o.h).fold(f64::MIN, f64::max) + GAP;
                }
                b
            } else {
                Self::free_spot(&s.key, s.w, s.h, &placed)
            };
            bx.id.clone_from(&s.key);
            placed.push(bx.clone());
            self.box_at.insert(s.key.clone(), bx.clone());
            let at = (bx.x - s.x0, bx.y - s.y0);
            self.region_at.insert(s.key.clone(), at);
            origin.insert(s.key.clone(), at);
        }

        let mut out = Layout::default();
        for s in slots {
            let (px, py) = origin[&s.key];
            if let Some(mut r) = s.region {
                r.rect.x = px;
                r.rect.y = py;
                let anchor = match (&r.title, r.links.is_empty()) {
                    (Some(t), false) => Rect { id: r.rect.id.clone(), x: px, y: py, w: t.w + t.x, h: TITLE },
                    _ => r.rect.clone(),
                };
                out.anchors.insert(r.rect.id.clone(), anchor);
                for row in &r.rows {
                    let pre = if row.prefix.is_empty() { 0.0 } else { 30.0 };
                    out.anchors.insert(row.id.clone(), Rect { id: row.id.clone(), x: px + row.x, y: py + row.y, w: row.w + pre, h: row.size + 8.0 });
                }
                out.regions.push(r);
            } else if let Some(mut d) = dia.take() {
                let frame = Rect { id: DIAGRAM.into(), x: px + d.x0, y: py + d.y0, w: d.w, h: d.h };
                for b in &mut d.nodes {
                    b.x += px;
                    b.y += py;
                    out.anchors.insert(b.id.clone(), b.clone());
                }
                for (b, _) in &mut d.groups {
                    b.x += px;
                    b.y += py;
                }
                for e in &mut d.edges {
                    for p in &mut e.points {
                        p[0] += px;
                        p[1] += py;
                    }
                }
                out.diagram = Some((frame, d));
            }
        }

        let mut topic_routes: HashMap<String, Route> = HashMap::new();
        for r in &out.regions {
            for link in &r.links {
                let mut link = link.clone();
                for p in &mut link.points {
                    p[0] += r.rect.x;
                    p[1] += r.rect.y;
                }
                if let Some(b) = &mut link.label_box {
                    b.x += r.rect.x;
                    b.y += r.rect.y;
                }
                topic_routes.insert(format!("{}>{}", link.from, link.to), link);
            }
        }
        for e in &g.edges {
            let in_diagram = ids.contains(&e.from) && ids.contains(&e.to);
            if !in_diagram && out.anchors.contains_key(&e.from) && out.anchors.contains_key(&e.to) {
                out.links.push((e.from.clone(), e.to.clone(), topic_routes.get(&format!("{}>{}", e.from, e.to)).cloned()));
            }
        }

        let mut all: Vec<&Rect> = out.anchors.values().chain(out.regions.iter().map(|r| &r.rect)).collect();
        if let Some((frame, _)) = &out.diagram {
            all.push(frame);
        }
        out.bounds = if all.is_empty() {
            Bounds { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0 }
        } else {
            Bounds {
                x0: all.iter().map(|b| b.x).fold(f64::MAX, f64::min),
                y0: all.iter().map(|b| b.y).fold(f64::MAX, f64::min),
                x1: all.iter().map(|b| b.x + b.w).fold(f64::MIN, f64::max),
                y1: all.iter().map(|b| b.y + b.h).fold(f64::MIN, f64::max),
            }
        };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::parse_ops;

    #[test]
    fn clicks_find_titles_rows_and_boxes_but_not_empty_column_space() {
        let mut g = Graph::default();
        g.apply(&parse_ops("T 「待办」\nL 「待办」 「压测」\nE 「甲」 -> 「乙」"));
        let l = StableLayout::default().layout(&g, &mut crate::sugiyama::first_diagram);
        let targets = l.targets();
        let id = |label: &str| g.nodes.iter().find(|n| n.label == label).unwrap().id.clone();
        for label in ["待办", "压测", "甲", "乙"] {
            let want = id(label);
            let b = targets.iter().find(|t| t.id == want).unwrap_or_else(|| panic!("{label} has no target"));
            let hit = target_at(&targets, b.x + b.w / 2.0, b.y + b.h / 2.0).unwrap();
            assert_eq!(hit.id, want, "{label}");
        }
        let column = l.regions.iter().find(|r| r.head.is_some()).unwrap();
        assert!(target_at(&targets, column.rect.x + column.rect.w - 1.0, column.rect.y + column.rect.h - 1.0).is_none());
    }
}
