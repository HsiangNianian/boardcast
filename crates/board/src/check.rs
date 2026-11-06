//! The layout checker. `hard` issues are layout engine defects, repaired by laying the board out
//! again. `soft` issues come from the board's content, and go back to the model as hints.
//! `boardcast-tools check` runs it from the command line and exits non-zero on a hard defect.

use crate::board_layout::{overlaps, Layout, Rect, StableLayout, P};
use crate::compose::compose;
use crate::glyphs::Glyphs;
use crate::graph::{Graph, Kind};
use crate::sugiyama::first_diagram;
use serde::Serialize;

/// Rows above this make a column longer than a typical window at full size.
const MAX_ROWS: usize = 7;
const MAX_LABEL_WIDTH: f64 = 12.0 * crate::board_layout::FONT;
/// Below this zoom, on a typical window, 20-unit text falls under 9 pixels.
const MIN_FIT_ZOOM: f64 = 0.45;
const TYPICAL_CANVAS: (f64, f64) = (960.0, 756.0);

#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub code: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub hard: Vec<Issue>,
    pub soft: Vec<Issue>,
    pub fit_zoom: f64,
}

impl Report {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("report serializes")
    }

    /// At most two hints, naming what to change; a long lecture would crowd out the speech.
    pub fn hint(&self) -> Option<String> {
        let useful: Vec<String> = self
            .soft
            .iter()
            .filter(|i| i.code != "label_no_clear_spot")
            .take(2)
            .map(|i| format!("{}（{}）", i.code, i.detail))
            .collect();
        (!useful.is_empty()).then(|| format!("排版提示：{}", useful.join("；")))
    }
}

fn runs_through(points: &[P], b: &Rect) -> bool {
    points.windows(2).any(|s| {
        (0..=40).any(|k| {
            let t = f64::from(k) / 40.0;
            let (x, y) = (s[0][0] + (s[1][0] - s[0][0]) * t, s[0][1] + (s[1][1] - s[0][1]) * t);
            x > b.x + 4.0 && x < b.x + b.w - 4.0 && y > b.y + 4.0 && y < b.y + b.h - 4.0
        })
    })
}

pub fn check(g: &Graph, l: &Layout, unclear: &[String]) -> Report {
    let mut r = Report::default();
    let name = |id: &str| g.nodes.iter().find(|n| n.id == id).map_or_else(|| id.to_string(), |n| n.label.clone());

    for n in &g.nodes {
        if !l.anchors.contains_key(&n.id) {
            r.hard.push(Issue { code: "unplaced_node", detail: n.label.clone() });
        }
    }
    let mut areas: Vec<&Rect> = l.regions.iter().map(|x| &x.rect).collect();
    if let Some((frame, _)) = &l.diagram {
        areas.push(frame);
    }
    for (i, a) in areas.iter().enumerate() {
        for b in &areas[i + 1..] {
            if overlaps(a, b, 0.0) {
                r.hard.push(Issue { code: "region_overlap", detail: format!("{} / {}", a.id, b.id) });
            }
        }
    }
    if let Some((_, d)) = &l.diagram {
        for (i, a) in d.nodes.iter().enumerate() {
            for b in &d.nodes[i + 1..] {
                if overlaps(a, b, 0.0) {
                    r.hard.push(Issue { code: "node_overlap", detail: format!("{} / {}", name(&a.id), name(&b.id)) });
                }
            }
        }
        for e in &d.edges {
            for b in d.nodes.iter().filter(|b| b.id != e.from && b.id != e.to) {
                if runs_through(&e.points, b) {
                    r.hard.push(Issue { code: "edge_through_node", detail: format!("{} -> {} / {}", name(&e.from), name(&e.to), name(&b.id)) });
                }
            }
        }
        for (frame, label) in &d.groups {
            for n in &d.nodes {
                let member = g.nodes.iter().any(|o| o.id == n.id && o.group.as_deref() == Some(frame.id.as_str()));
                if !member && overlaps(n, frame, 0.0) {
                    r.hard.push(Issue { code: "frame_stranger", detail: format!("{} / {label}", name(&n.id)) });
                }
            }
        }
    }

    for reg in &l.regions {
        if reg.rows.len() > MAX_ROWS && reg.head.is_some() {
            let head = reg.head.as_deref().map(&name).unwrap_or_default();
            r.soft.push(Issue { code: "crowded_topic", detail: format!("{head} 有 {} 条", reg.rows.len()) });
        }
        if reg.head.is_some() && reg.rows.is_empty() && !g.edges.iter().any(|e| Some(&e.from) == reg.head.as_ref() || Some(&e.to) == reg.head.as_ref()) {
            r.soft.push(Issue { code: "empty_topic", detail: reg.head.as_deref().map(&name).unwrap_or_default() });
        }
    }
    for n in &g.nodes {
        if crate::board_layout::text_width(&n.label, crate::board_layout::FONT) > MAX_LABEL_WIDTH {
            r.soft.push(Issue { code: "long_label", detail: n.label.clone() });
        }
    }
    let is_topic = |id: &str| g.nodes.iter().any(|n| n.id == id && n.kind == Kind::Topic);
    for e in &g.edges {
        if is_topic(&e.from) || is_topic(&e.to) {
            r.soft.push(Issue { code: "topic_in_flow", detail: format!("{} -> {}", name(&e.from), name(&e.to)) });
        }
        if e.from < e.to && g.edges.iter().any(|o| o.from == e.to && o.to == e.from) {
            r.soft.push(Issue { code: "two_way", detail: format!("{} <-> {}", name(&e.from), name(&e.to)) });
        }
    }
    for label in unclear {
        r.soft.push(Issue { code: "label_no_clear_spot", detail: label.clone() });
    }
    let (w, h) = TYPICAL_CANVAS;
    let b = &l.bounds;
    r.fit_zoom = (w / (b.x1 - b.x0 + 120.0)).min(h / (b.y1 - b.y0 + 120.0)).min(1.0);
    if r.fit_zoom < MIN_FIT_ZOOM {
        r.soft.push(Issue { code: "board_too_large", detail: format!("适配缩放 {:.0}%", r.fit_zoom * 100.0) });
    }
    r
}

/// Check a board and, on a hard defect, lay it out again from scratch. Returns both reports.
pub fn check_and_repair(g: &Graph, board: &mut StableLayout, glyphs: &mut Glyphs) -> (Report, Option<Report>) {
    let l = board.layout(g, &mut first_diagram);
    let (_, unclear) = compose(g, &l, glyphs);
    let report = check(g, &l, &unclear);
    if report.hard.is_empty() {
        return (report, None);
    }
    *board = StableLayout::default();
    let l = board.layout(g, &mut first_diagram);
    let (_, unclear) = compose(g, &l, glyphs);
    let repaired = check(g, &l, &unclear);
    (report, Some(repaired))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::parse_ops;

    fn report(ops: &str) -> Report {
        let mut g = Graph::default();
        g.apply(&parse_ops(ops));
        let l = StableLayout::default().layout(&g, &mut first_diagram);
        check(&g, &l, &[])
    }

    fn codes(list: &[Issue]) -> Vec<&'static str> {
        list.iter().map(|i| i.code).collect()
    }

    #[test]
    fn a_clean_board_has_nothing_to_report() {
        let r = report("E 「用户」 -> 「网关」\nE 「网关」 -> 「登录」\nT 「待办」\nL 「待办」 「压测」");
        assert!(r.hard.is_empty(), "{r:?}");
        assert!(r.soft.is_empty(), "{r:?}");
    }

    #[test]
    fn content_problems_become_hints() {
        let topic = report("T 「登录」\nE 「登录」 -> 「网关」");
        assert!(codes(&topic.soft).contains(&"topic_in_flow"), "{topic:?}");
        assert!(topic.hint().unwrap().starts_with("排版提示"));
        assert!(codes(&report("E 「甲」 -> 「乙」\nE 「乙」 -> 「甲」").soft).contains(&"two_way"));
        assert!(codes(&report("T 「空」").soft).contains(&"empty_topic"));
        let many: String = (0..9).map(|i| format!("L 「待办」 「事项{i}」\n")).collect();
        assert!(codes(&report(&format!("T 「待办」\n{many}")).soft).contains(&"crowded_topic"));
        assert!(codes(&report("T 「这是一个特别特别特别特别特别长的名字」").soft).contains(&"long_label"));
    }

    /// Random boards built one line at a time are the hardest case for incremental layout. Whatever
    /// it gets wrong must be caught, and laying the board out again must fix it.
    #[test]
    fn repair_clears_every_defect_the_incremental_layout_causes() {
        let goldens: serde_json::Value = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        let (mut steps, mut defective, mut left) = (0, 0, 0);
        let mut codes_seen = std::collections::BTreeMap::new();
        for case in goldens["stress"].as_array().unwrap() {
            let mut g = Graph::default();
            let mut board = StableLayout::default();
            for line in case["ops"].as_str().unwrap().lines() {
                g.apply(&parse_ops(line));
                let l = board.layout(&g, &mut first_diagram);
                let found = check(&g, &l, &[]);
                steps += 1;
                if found.hard.is_empty() {
                    continue;
                }
                defective += 1;
                for issue in &found.hard {
                    *codes_seen.entry(issue.code).or_insert(0) += 1;
                }
                board = StableLayout::default();
                let again = board.layout(&g, &mut first_diagram);
                left += check(&g, &again, &[]).hard.len();
            }
        }
        println!("REPAIR steps {steps}, defective {defective}, codes {codes_seen:?}, left after repair {left}");
        assert_eq!(left, 0, "defects survive a full layout");
        assert!(defective > 0, "the incremental layout was never stressed, so this proves nothing");
    }

    #[test]
    fn hand_made_defects_are_detected() {
        use crate::board_layout::{Bounds, Diagram, Route};
        let mut g = Graph::default();
        g.apply(&parse_ops("E 「甲」 -> 「乙」\nN 「丙」\nG 「组」 「甲」"));
        let rect = |id: &str, x, y| Rect { id: id.to_string(), x, y, w: 100.0, h: 60.0 };
        let id = |label: &str| g.nodes.iter().find(|n| n.label == label).unwrap().id.clone();
        let (a, b, c) = (id("甲"), id("乙"), id("丙"));
        let nodes = vec![rect(&a, 0.0, 0.0), rect(&b, 400.0, 0.0), rect(&c, 200.0, 0.0)];
        let l = Layout {
            diagram: Some((
                rect("__diagram", 0.0, 0.0),
                Diagram {
                    nodes: nodes.clone(),
                    edges: vec![Route { from: a.clone(), to: b, points: vec![[100.0, 30.0], [400.0, 30.0]], label_box: None }],
                    groups: vec![(Rect { id: g.groups[0].id.clone(), x: 0.0, y: 0.0, w: 250.0, h: 60.0 }, "组".into())],
                    ..Diagram::default()
                },
            )),
            anchors: nodes.iter().map(|n| (n.id.clone(), n.clone())).collect(),
            bounds: Bounds { x0: 0.0, y0: 0.0, x1: 500.0, y1: 60.0 },
            ..Layout::default()
        };
        let found = codes(&check(&g, &l, &[]).hard);
        assert!(found.contains(&"edge_through_node"), "{found:?}");
        assert!(found.contains(&"frame_stranger"), "{found:?}");
    }
}
