//! The record behind replay: what changed each time the board changed, ported from src/engine/steps.ts.
//! Steps are written into meeting files, so the format must stay readable by the older app.

use crate::graph::{Edge, Graph, Group, Node};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Part<T> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub set: Vec<T>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub del: Vec<String>,
    /// The full new order, when removing and appending cannot reproduce it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Diff {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Part<Node>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<Part<Edge>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<Part<Group>>,
    #[serde(rename = "nextNode", default, skip_serializing_if = "Option::is_none")]
    pub next_node: Option<u32>,
    #[serde(rename = "nextGroup", default, skip_serializing_if = "Option::is_none")]
    pub next_group: Option<u32>,
    /// Kept as written: only imported pages carry it, and those are not edited here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underlay: Option<Value>,
}

fn edge_key(e: &Edge) -> String {
    format!("{}>{}", e.from, e.to)
}

fn diff_part<T: Clone + PartialEq>(a: &[T], b: &[T], key: impl Fn(&T) -> String) -> Option<Part<T>> {
    let set: Vec<T> = b.iter().filter(|x| a.iter().find(|o| key(o) == key(x)) != Some(*x)).cloned().collect();
    let del: Vec<String> = a.iter().map(&key).filter(|k| !b.iter().any(|x| &key(x) == k)).collect();
    let real: Vec<String> = b.iter().map(&key).collect();
    let kept = a.iter().map(&key).filter(|k| real.contains(k));
    let added = real.iter().filter(|k| !a.iter().any(|x| &key(x) == *k)).cloned();
    let guess: Vec<String> = kept.chain(added).collect();
    let order = (guess != real).then_some(real);
    (!set.is_empty() || !del.is_empty() || order.is_some()).then_some(Part { set, del, order })
}

fn patch_part<T: Clone>(a: &[T], part: Option<&Part<T>>, key: impl Fn(&T) -> String) -> Vec<T> {
    let Some(part) = part else { return a.to_vec() };
    let mut rows: Vec<(String, T)> = a.iter().filter(|x| !part.del.contains(&key(x))).map(|x| (key(x), x.clone())).collect();
    for x in &part.set {
        match rows.iter_mut().find(|(k, _)| *k == key(x)) {
            Some(row) => row.1 = x.clone(),
            None => rows.push((key(x), x.clone())),
        }
    }
    match &part.order {
        Some(order) => order.iter().filter_map(|k| rows.iter().find(|(o, _)| o == k).map(|(_, x)| x.clone())).collect(),
        None => rows.into_iter().map(|(_, x)| x).collect(),
    }
}

/// What differs between two boards; `None` if nothing does.
pub fn diff(a: &Graph, b: &Graph) -> Option<Diff> {
    let d = Diff {
        nodes: diff_part(&a.nodes, &b.nodes, |n: &Node| n.id.clone()),
        edges: diff_part(&a.edges, &b.edges, edge_key),
        groups: diff_part(&a.groups, &b.groups, |g: &Group| g.id.clone()),
        next_node: (a.next_node != b.next_node).then_some(b.next_node),
        next_group: (a.next_group != b.next_group).then_some(b.next_group),
        underlay: None,
    };
    (d != Diff::default()).then_some(d)
}

pub fn patch(g: &Graph, d: &Diff) -> Graph {
    let mut out = g.clone();
    out.nodes = patch_part(&g.nodes, d.nodes.as_ref(), |n: &Node| n.id.clone());
    out.edges = patch_part(&g.edges, d.edges.as_ref(), edge_key);
    out.groups = patch_part(&g.groups, d.groups.as_ref(), |x: &Group| x.id.clone());
    if let Some(n) = d.next_node {
        out.next_node = n;
    }
    if let Some(n) = d.next_group {
        out.next_group = n;
    }
    out
}

/// Replay shows settled ink, so pencil drafts are left out.
pub fn ink_only(g: &Graph) -> Graph {
    let mut out = g.clone();
    out.nodes.retain(|n| n.ink != "draft");
    let alive: Vec<String> = out.nodes.iter().map(|n| n.id.clone()).collect();
    out.edges.retain(|e| e.ink != "draft" && alive.contains(&e.from) && alive.contains(&e.to));
    out
}

/// The board after every stored step. Steps this code cannot read are skipped.
pub fn replay(steps: &[Value]) -> Graph {
    let mut g = Graph::default();
    for step in steps {
        if let Ok(d) = serde_json::from_value::<Diff>(step["d"].clone()) {
            g = patch(&g, &d);
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::parse_ops;
    use serde_json::json;

    #[test]
    fn replaying_the_diffs_rebuilds_every_recorded_board() {
        let goldens: Value = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        let mut steps: Vec<Value> = vec![];
        let (mut g, mut file) = (Graph::default(), String::new());
        for s in goldens["steps"].as_array().unwrap() {
            if s["file"].as_str().unwrap() != file {
                file = s["file"].as_str().unwrap().to_string();
                g = Graph::default();
                steps.clear();
            }
            let before = g.clone();
            g.apply(&parse_ops(s["ops"].as_str().unwrap()));
            if let Some(d) = diff(&before, &g) {
                steps.push(json!({ "t": 1, "line": 1, "d": d }));
            }
            let replayed = replay(&steps);
            assert_eq!(replayed.nodes, g.nodes, "{file}");
            assert_eq!(replayed.edges, g.edges, "{file}");
            assert_eq!(replayed.groups, g.groups, "{file}");
        }
    }

    #[test]
    fn identical_boards_have_no_diff() {
        let mut g = Graph::default();
        g.apply(&parse_ops("E 「甲」 -> 「乙」"));
        assert_eq!(diff(&g, &g.clone()), None);
    }

    #[test]
    fn a_reordering_is_recorded() {
        let mut a = Graph::default();
        a.apply(&parse_ops("T 「甲」\nT 「乙」"));
        let mut b = a.clone();
        b.nodes.reverse();
        let d = diff(&a, &b).unwrap();
        assert_eq!(patch(&a, &d).nodes, b.nodes);
    }
}
