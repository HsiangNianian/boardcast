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

/// The board after the first `count` steps.
pub fn replay_to(steps: &[Value], count: usize) -> Graph {
    replay(&steps[..count.min(steps.len())])
}

/// `3:07`, or `1:02:45` for an hour or more.
pub fn clock(ms: f64) -> String {
    let s = (ms / 1000.0).round().max(0.0) as u64;
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h > 0 {
        format!("{h}:{m:02}:{:02}", s % 60)
    } else {
        format!("{m}:{:02}", s % 60)
    }
}

pub const SPEEDS: [f64; 3] = [1.0, 2.0, 4.0];

/// Where a replay has got to. It holds no board: the steps are passed in, as they belong to the meeting.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    /// How many steps have been played; 0 is the empty board.
    pub at: usize,
    pub total: usize,
    pub playing: bool,
    pub speed: f64,
}

fn time_of(step: &Value) -> f64 {
    step["t"].as_f64().unwrap_or(0.0)
}

impl Player {
    pub fn new(total: usize) -> Self {
        Self { at: 0, total, playing: true, speed: SPEEDS[0] }
    }

    /// Move to a step, stopping at the end. Says how far it moved, or `None` if it did not.
    pub fn seek(&mut self, at: usize) -> Option<isize> {
        let at = at.min(self.total);
        if at == self.at {
            return None;
        }
        let moved = at as isize - self.at as isize;
        self.at = at;
        self.playing = self.playing && at < self.total;
        Some(moved)
    }

    /// Play or pause. Playing from the end starts over, and then it returns true.
    pub fn toggle(&mut self) -> bool {
        if self.playing {
            self.playing = false;
            return false;
        }
        self.playing = true;
        if self.at >= self.total {
            self.at = 0;
            return true;
        }
        false
    }

    pub fn next_speed(&mut self) {
        let i = SPEEDS.iter().position(|s| *s == self.speed).unwrap_or(0);
        self.speed = SPEEDS[(i + 1) % SPEEDS.len()];
    }

    /// How long before the next step: the meeting's own pace, with long silences cut to 3 seconds and
    /// at least 0.9 for the writing to show, divided by the speed.
    pub fn wait_ms(&self, steps: &[Value]) -> f64 {
        if self.at == 0 {
            return 300.0;
        }
        let gap = match (steps.get(self.at - 1), steps.get(self.at)) {
            (Some(a), Some(b)) => time_of(b) - time_of(a),
            _ => 0.0,
        };
        gap.max(900.0).min(3000.0) / self.speed
    }

    /// How far into the meeting this step was, in milliseconds.
    pub fn elapsed_ms(&self, steps: &[Value]) -> f64 {
        match (steps.first(), self.at.checked_sub(1).and_then(|i| steps.get(i))) {
            (Some(first), Some(now)) => time_of(now) - time_of(first),
            _ => 0.0,
        }
    }

    pub fn duration_ms(steps: &[Value]) -> f64 {
        match (steps.first(), steps.last()) {
            (Some(first), Some(last)) => time_of(last) - time_of(first),
            _ => 0.0,
        }
    }

    /// How many transcript lines had been said by this step.
    pub fn lines_said(&self, steps: &[Value]) -> usize {
        self.at.checked_sub(1).and_then(|i| steps.get(i)).and_then(|s| s["line"].as_u64()).unwrap_or(0) as usize
    }
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

    fn recorded() -> Vec<Value> {
        let boards = ["T 「甲」", "T 「甲」\nT 「乙」", "T 「甲」\nT 「乙」\nT 「丙」"];
        let mut before = Graph::default();
        let mut steps = vec![];
        for (i, ops) in boards.iter().enumerate() {
            let mut g = Graph::default();
            g.apply(&parse_ops(ops));
            let t = [1000, 1200, 61000][i];
            steps.push(json!({ "t": t, "line": i + 1, "d": diff(&before, &g).unwrap() }));
            before = g;
        }
        steps
    }

    #[test]
    fn a_replay_can_stop_at_any_step() {
        let steps = recorded();
        assert!(replay_to(&steps, 0).nodes.is_empty());
        assert_eq!(replay_to(&steps, 2).nodes.len(), 2);
        assert_eq!(replay_to(&steps, 99).nodes.len(), 3);
    }

    #[test]
    fn a_player_keeps_the_meetings_pace_within_limits() {
        let steps = recorded();
        let mut p = Player::new(3);
        assert_eq!(p.wait_ms(&steps), 300.0);
        p.seek(1);
        assert_eq!(p.wait_ms(&steps), 900.0, "too quick to see is slowed down");
        p.seek(2);
        assert_eq!(p.wait_ms(&steps), 3000.0, "a long silence is cut");
        p.next_speed();
        assert_eq!(p.wait_ms(&steps), 1500.0);
        p.next_speed();
        p.next_speed();
        assert_eq!(p.speed, 1.0);
    }

    #[test]
    fn a_player_stops_at_the_end_and_starts_over_from_it() {
        let mut p = Player::new(3);
        assert_eq!(p.seek(0), None);
        assert_eq!(p.seek(2), Some(2));
        assert!(p.playing);
        assert_eq!(p.seek(99), Some(1));
        assert!(!p.playing);
        assert_eq!(p.at, 3);
        assert!(p.toggle(), "playing from the end starts over");
        assert_eq!(p.at, 0);
        assert!(p.playing);
        assert_eq!(p.seek(1), Some(1));
        assert!(!p.toggle());
        assert!(!p.playing);
        assert_eq!(p.seek(0), Some(-1));
    }

    #[test]
    fn a_player_reads_time_and_lines_from_the_steps() {
        let steps = recorded();
        let mut p = Player::new(3);
        assert_eq!((p.elapsed_ms(&steps), p.lines_said(&steps)), (0.0, 0));
        p.seek(3);
        assert_eq!((p.elapsed_ms(&steps), p.lines_said(&steps)), (60000.0, 3));
        assert_eq!(Player::duration_ms(&steps), 60000.0);
        assert_eq!(Player::duration_ms(&[]), 0.0);
    }

    #[test]
    fn clocks_read_as_minutes_or_hours() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(187_000.0), "3:07");
        assert_eq!(clock(3_765_000.0), "1:02:45");
        assert_eq!(clock(-5.0), "0:00");
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
