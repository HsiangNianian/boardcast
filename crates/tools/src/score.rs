//! Compares the board a model drew with the hand-written answer, ported from eval/score.ts so scores stay
//! comparable. Names match loosely (aliases, one containing the other), since "login API" and "login
//! service" look the same on a whiteboard.

use boardcast_board::graph::{is_han, norm, Graph, Kind, Node};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap, HashSet};

type Names = Vec<String>;
type Pair = (String, String);

#[derive(Deserialize, Default, Clone)]
pub struct Topic {
    pub title: Names,
    pub items: Vec<Names>,
}

/// The answer for a case. Every field is optional; what is left out is not scored.
#[derive(Deserialize, Default, Clone)]
pub struct Gold {
    /// Flow-chart nodes; the first name is the answer, the rest also count.
    pub nodes: Option<Vec<Names>>,
    pub edges: Option<Vec<Pair>>,
    /// The members of each should share one group.
    pub groups: Option<Vec<Names>>,
    /// These edges count drawn either way.
    pub either_direction: Option<Vec<Pair>>,
    /// The first of each is the answer; drawing any of the others counts as it.
    pub alt_edges: Option<Vec<Vec<Pair>>>,
    /// Neither rewarded nor penalised.
    pub optional_nodes: Option<Vec<Names>>,
    pub optional_edges: Option<Vec<Pair>>,
    /// Wrong if present and not struck.
    pub forbidden_nodes: Option<Vec<String>>,
    pub forbidden_edges: Option<Vec<Pair>>,
    /// Outline boards: topics, and the items that belong under them at any depth.
    pub topics: Option<Vec<Topic>>,
    pub struck: Option<Vec<Names>>,
    /// The user took the model's strike off: striking it again misses a mark.
    pub not_struck: Option<Vec<Names>>,
    /// Starred, circled or underlined all count.
    pub emphasized: Option<Vec<Names>>,
    pub not_emphasized: Option<Vec<Names>>,
    /// Kept as a question or formula; any one keyword is enough.
    pub questions: Option<Vec<Names>>,
    pub formulas: Option<Vec<Names>>,
}

#[derive(Debug, Default)]
pub struct Score {
    pub node_p: f64,
    pub node_r: f64,
    pub edge_p: f64,
    pub edge_r: f64,
    pub topic_r: Option<f64>,
    pub item_p: Option<f64>,
    pub item_r: Option<f64>,
    /// Whole-node strikes; with `struck` present but empty, this catches striking by mistake.
    pub strike_p: Option<f64>,
    pub strike_r: Option<f64>,
    /// Strikes, emphasis, questions and formulas found.
    pub mark_acc: Option<f64>,
    pub group_acc: Option<f64>,
    pub violations: Vec<String>,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
}

pub fn f1(p: f64, r: f64) -> f64 {
    if p + r == 0.0 {
        0.0
    } else {
        2.0 * p * r / (p + r)
    }
}

/// Lower-case words of a label with no Han in it; anything but letters and digits splits words.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// English labels match word by word: "Auth-service" equals "Auth service", "ClickHouse" equals "Click
/// House", and a partial match has to be whole consecutive words ("Auth" in "Auth service", not "API" in
/// "Capital").
fn same_words(a: &str, b: &str) -> u8 {
    let (x, y) = (words(a), words(b));
    if x.is_empty() || y.is_empty() {
        return 0;
    }
    if x.concat() == y.concat() {
        return 2;
    }
    let (s, l) = if x.len() <= y.len() {
        (&x, &y)
    } else {
        (&y, &x)
    };
    let contained =
        (0..=l.len() - s.len()).any(|i| s.iter().enumerate().all(|(j, w)| &l[i + j] == w));
    u8::from(contained)
}

/// 2 for the same name, 1 for one inside the other, 0 for different.
fn same(a: &str, b: &str) -> u8 {
    if !a.chars().chain(b.chars()).any(is_han) {
        return same_words(a, b);
    }
    let (x, y) = (norm(a), norm(b));
    if x.is_empty() || y.is_empty() {
        return 0;
    }
    if x == y {
        return 2;
    }
    let (s, l) = if x.chars().count() <= y.chars().count() {
        (&x, &y)
    } else {
        (&y, &x)
    };
    u8::from(s.chars().count() >= 2 && l.contains(s.as_str()))
}

fn names(n: &Node) -> Vec<&str> {
    std::iter::once(n.label.as_str())
        .chain(n.aliases.iter().map(String::as_str))
        .collect()
}

fn best(pred: &[&str], gold: &[String]) -> u8 {
    pred.iter()
        .flat_map(|p| gold.iter().map(move |g| same(p, g)))
        .max()
        .unwrap_or(0)
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 {
        1.0
    } else {
        a as f64 / b as f64
    }
}

const STRIKE: &str = "划掉";
const EMPHASIS: [&str; 3] = ["星标", "圈出", "下划线"];

#[derive(Clone, Copy, PartialEq)]
enum Type {
    Node,
    Topic,
    Item,
    Optional,
}

struct Entity<'a> {
    key: &'a str,
    names: &'a [String],
    kind: Type,
    topic: Option<&'a str>,
}

pub fn score(g: &Graph, gold: &Gold) -> Score {
    let mut entities: Vec<Entity> = vec![];
    for n in gold.nodes.iter().flatten() {
        entities.push(Entity {
            key: &n[0],
            names: n,
            kind: Type::Node,
            topic: None,
        });
    }
    for t in gold.topics.iter().flatten() {
        entities.push(Entity {
            key: &t.title[0],
            names: &t.title,
            kind: Type::Topic,
            topic: None,
        });
        for it in &t.items {
            entities.push(Entity {
                key: &it[0],
                names: it,
                kind: Type::Item,
                topic: Some(&t.title[0]),
            });
        }
    }
    for o in gold.optional_nodes.iter().flatten() {
        entities.push(Entity {
            key: &o[0],
            names: o,
            kind: Type::Optional,
            topic: None,
        });
    }

    // Exact matches first, then containment. Questions, formulas and notes take no part.
    let attach = |n: &Node| matches!(n.kind, Kind::Question | Kind::Formula | Kind::Note);
    let mut map: HashMap<&str, usize> = HashMap::new(); // predicted id -> entity
    let mut taken: HashSet<&str> = HashSet::new();
    for level in [2, 1] {
        for (e, ent) in entities.iter().enumerate() {
            if taken.contains(ent.key) {
                continue;
            }
            if let Some(hit) = g.nodes.iter().find(|n| {
                !attach(n)
                    && !map.contains_key(n.id.as_str())
                    && best(&names(n), ent.names) >= level
            }) {
                map.insert(&hit.id, e);
                taken.insert(ent.key);
            }
        }
    }
    let struck: HashSet<&str> = g
        .nodes
        .iter()
        .filter(|n| n.marks.iter().any(|m| m == STRIKE))
        .map(|n| n.id.as_str())
        .collect();

    let mut violations = vec![];
    for n in &g.nodes {
        for fb in gold.forbidden_nodes.iter().flatten() {
            // A rename keeps the old name as an alias, rightly, so only the current name counts; struck is handled.
            if same(&n.label, fb) == 2 && !struck.contains(n.id.as_str()) {
                violations.push(format!("should not appear: {}", n.label));
            }
        }
    }

    let gold_nodes: Vec<&Entity> = entities.iter().filter(|e| e.kind == Type::Node).collect();
    let node_tp = gold_nodes.iter().filter(|e| taken.contains(e.key)).count();
    let pred_nodes: Vec<&Node> = g
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Node && !struck.contains(n.id.as_str()))
        .collect();
    let pred_node_hit = pred_nodes
        .iter()
        .filter(|n| {
            map.get(n.id.as_str())
                .is_some_and(|&e| entities[e].kind == Type::Node)
        })
        .count();

    // Topics and items: an item counts only under the right topic, at any depth.
    let topics: Vec<&Entity> = entities.iter().filter(|e| e.kind == Type::Topic).collect();
    let items: Vec<&Entity> = entities.iter().filter(|e| e.kind == Type::Item).collect();
    let id_of = |key: &str| {
        map.iter()
            .find(|(_, &e)| entities[e].key == key)
            .map(|(id, _)| *id)
    };
    let ancestors = |id: &str| {
        let mut out: HashSet<&str> = HashSet::new();
        let mut cur = g.nodes.iter().find(|n| n.id == id);
        while let Some(parent) = cur.and_then(|n| n.parent.as_deref()) {
            if !out.insert(parent) {
                break;
            }
            cur = g.nodes.iter().find(|n| n.id == parent);
        }
        out
    };
    let (mut item_tp, mut misplaced) = (0, vec![]);
    for it in &items {
        match (id_of(it.key), it.topic.and_then(id_of)) {
            (Some(id), Some(tid)) if ancestors(id).contains(tid) => item_tp += 1,
            (Some(_), _) => misplaced.push(it.key),
            _ => {}
        }
    }
    let pred_items: Vec<&Node> = g.nodes.iter().filter(|n| n.kind == Kind::Item).collect();

    // Strikes on their own, one to one, so repeated or mistaken strikes count against.
    let strike_gold = gold.struck.clone().unwrap_or_default();
    let strike_pred: Vec<&Node> = g
        .nodes
        .iter()
        .filter(|n| struck.contains(n.id.as_str()))
        .collect();
    let (mut gold_hit, mut pred_hit, mut strike_tp) = (HashSet::new(), HashSet::new(), 0);
    for level in [2, 1] {
        for (i, want) in strike_gold.iter().enumerate() {
            if gold_hit.contains(&i) {
                continue;
            }
            if let Some(hit) = strike_pred
                .iter()
                .find(|n| !pred_hit.contains(n.id.as_str()) && best(&names(n), want) >= level)
            {
                gold_hit.insert(i);
                pred_hit.insert(hit.id.as_str());
                strike_tp += 1;
            }
        }
    }
    let score_strike = gold.struck.is_some() || gold.not_struck.is_some();

    // Marks.
    let (mut mark_hit, mut mark_all, mut mark_miss) = (0, 0, vec![]);
    let emphasized = |n: &Node| n.marks.iter().any(|m| EMPHASIS.contains(&m.as_str()));
    let mut has = |want: &Names, ok: &dyn Fn(&Node) -> bool, label: &str| {
        mark_all += 1;
        if g.nodes.iter().any(|n| ok(n) && best(&names(n), want) >= 1) {
            mark_hit += 1;
        } else {
            mark_miss.push(format!("{label}: {}", want[0]));
        }
    };
    let is_struck = |n: &Node| n.marks.iter().any(|m| m == STRIKE);
    for s in gold.struck.iter().flatten() {
        has(s, &is_struck, "not struck");
    }
    for s in gold.emphasized.iter().flatten() {
        has(s, &emphasized, "not emphasized");
    }
    for q in gold.questions.iter().flatten() {
        has(
            q,
            &|n: &Node| n.kind == Kind::Question,
            "not kept as a question",
        );
    }
    for f in gold.formulas.iter().flatten() {
        has(
            f,
            &|n: &Node| n.kind == Kind::Formula,
            "not kept as a formula",
        );
    }
    for s in gold.not_struck.iter().flatten() {
        mark_all += 1;
        if g.nodes
            .iter()
            .any(|n| is_struck(n) && best(&names(n), s) >= 1)
        {
            mark_miss.push(format!("struck by mistake: {}", s[0]));
        } else {
            mark_hit += 1;
        }
    }
    for s in gold.not_emphasized.iter().flatten() {
        mark_all += 1;
        if g.nodes
            .iter()
            .any(|n| emphasized(n) && best(&names(n), s) >= 1)
        {
            mark_miss.push(format!("emphasis in the wrong place: {}", s[0]));
        } else {
            mark_hit += 1;
        }
    }

    // Edges compare by the answer's names; lines touching something struck don't count.
    let canon = |a: &str, b: &str| -> String {
        for grp in gold.alt_edges.iter().flatten() {
            if grp.iter().any(|(x, y)| x == a && y == b) {
                return format!("{}>{}", grp[0].0, grp[0].1);
            }
        }
        if let Some((x, y)) = gold
            .either_direction
            .iter()
            .flatten()
            .find(|(x, y)| (x == a && y == b) || (x == b && y == a))
        {
            return format!("{x}>{y}");
        }
        format!("{a}>{b}")
    };
    let gold_edges: BTreeSet<String> = gold
        .edges
        .iter()
        .flatten()
        .map(|(a, b)| canon(a, b))
        .collect();
    let optional: HashSet<String> = gold
        .optional_edges
        .iter()
        .flatten()
        .map(|(a, b)| format!("{a}>{b}"))
        .collect();
    let forbidden: HashSet<String> = gold
        .forbidden_edges
        .iter()
        .flatten()
        .map(|(a, b)| format!("{a}>{b}"))
        .collect();
    let label_of = |id: &str| -> String {
        match map.get(id) {
            Some(&e) => entities[e].key.to_string(),
            None => g
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map_or_else(|| id.to_string(), |n| n.label.clone()),
        }
    };
    let mut pred_edges: BTreeSet<String> = BTreeSet::new();
    for e in &g.edges {
        if struck.contains(e.from.as_str()) || struck.contains(e.to.as_str()) {
            continue;
        }
        let (a, b) = (label_of(&e.from), label_of(&e.to));
        let raw = format!("{a}>{b}");
        if forbidden.contains(&raw) {
            violations.push(format!("edge should not appear: {a} → {b}"));
        }
        if optional.contains(&raw) {
            continue;
        }
        pred_edges.insert(canon(&a, &b));
    }
    let edge_tp = pred_edges
        .iter()
        .filter(|e| gold_edges.contains(*e))
        .count();

    // Groups: whether each set of answer members landed in one group.
    let group_acc = gold
        .groups
        .as_ref()
        .filter(|gs| !gs.is_empty())
        .map(|groups| {
            let ok = groups
                .iter()
                .filter(|members| {
                    let gs: Vec<Option<&String>> = members
                        .iter()
                        .map(|m| {
                            g.nodes
                                .iter()
                                .find(|n| {
                                    map.get(n.id.as_str())
                                        .is_some_and(|&e| entities[e].key == m)
                                })
                                .and_then(|n| n.group.as_ref())
                        })
                        .collect();
                    gs.iter().all(|x| x.is_some() && *x == gs[0])
                })
                .count();
            ok as f64 / groups.len() as f64
        });

    let arrow = |s: &str| s.replacen('>', " → ", 1);
    let mut missing: Vec<String> = vec![];
    missing.extend(
        gold_nodes
            .iter()
            .filter(|e| !taken.contains(e.key))
            .map(|e| format!("node {}", e.key)),
    );
    missing.extend(
        topics
            .iter()
            .filter(|e| !taken.contains(e.key))
            .map(|e| format!("topic {}", e.key)),
    );
    missing.extend(
        items
            .iter()
            .filter(|e| !taken.contains(e.key))
            .map(|e| format!("item {}", e.key)),
    );
    missing.extend(
        misplaced
            .iter()
            .map(|k| format!("item in the wrong place {k}")),
    );
    missing.extend(
        gold_edges
            .iter()
            .filter(|e| !pred_edges.contains(*e))
            .map(|e| format!("edge {}", arrow(e))),
    );
    missing.extend(mark_miss);
    let mut extra: Vec<String> = vec![];
    extra.extend(
        pred_nodes
            .iter()
            .filter(|n| !map.contains_key(n.id.as_str()))
            .map(|n| format!("node {}", n.label)),
    );
    extra.extend(
        pred_items
            .iter()
            .filter(|n| !map.contains_key(n.id.as_str()))
            .map(|n| format!("item {}", n.label)),
    );
    extra.extend(
        pred_edges
            .iter()
            .filter(|e| !gold_edges.contains(*e))
            .map(|e| format!("edge {}", arrow(e))),
    );

    Score {
        node_p: ratio(pred_node_hit, pred_nodes.len()),
        node_r: ratio(node_tp, gold_nodes.len()),
        edge_p: ratio(edge_tp, pred_edges.len()),
        edge_r: ratio(edge_tp, gold_edges.len()),
        topic_r: (!topics.is_empty()).then(|| {
            topics.iter().filter(|t| taken.contains(t.key)).count() as f64 / topics.len() as f64
        }),
        item_p: (!items.is_empty()).then(|| ratio(item_tp, pred_items.len())),
        item_r: (!items.is_empty()).then(|| item_tp as f64 / items.len() as f64),
        strike_p: score_strike.then(|| ratio(strike_tp, strike_pred.len())),
        strike_r: score_strike.then(|| ratio(strike_tp, strike_gold.len())),
        mark_acc: (mark_all > 0).then(|| mark_hit as f64 / mark_all as f64),
        group_acc,
        violations,
        missing,
        extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Every recorded demo's final board, scored by eval/score.ts before it was removed: the port must agree.
    #[test]
    fn scores_match_the_typescript_scorer() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let expected: serde_json::Map<String, Value> =
            serde_json::from_str(include_str!("../fixtures/score-goldens.json")).unwrap();
        let mut cases: Vec<Value> = vec![];
        for set in ["cases.json", "holdout.json", "pointing.json"] {
            cases.extend(
                serde_json::from_slice::<Vec<Value>>(
                    &std::fs::read(format!("{root}/eval/{set}")).unwrap(),
                )
                .unwrap(),
            );
        }
        assert!(expected.len() >= 20);
        for (id, want) in &expected {
            let demo: Value = serde_json::from_slice(
                &std::fs::read(format!("{root}/assets/demos/{id}.json")).unwrap(),
            )
            .unwrap();
            let graph: Graph = serde_json::from_value(demo["graph"].clone()).unwrap();
            let case = cases.iter().find(|c| c["id"] == id.as_str()).unwrap();
            let gold: Gold = serde_json::from_value(case["gold"].clone()).unwrap();
            let s = score(&graph, &gold);
            let close = |got: Option<f64>, key: &str| {
                let want = want[key].as_f64();
                assert!(
                    got.zip(want)
                        .map_or(got.is_none() && want.is_none(), |(a, b)| (a - b).abs()
                            < 1e-9),
                    "{id} {key}: {got:?} vs {want:?}"
                );
            };
            for (key, got) in [
                ("node_p", s.node_p),
                ("node_r", s.node_r),
                ("edge_p", s.edge_p),
                ("edge_r", s.edge_r),
            ] {
                close(Some(got), key);
            }
            for (key, got) in [
                ("topic_r", s.topic_r),
                ("item_p", s.item_p),
                ("item_r", s.item_r),
                ("strike_p", s.strike_p),
                ("strike_r", s.strike_r),
                ("mark_acc", s.mark_acc),
                ("group_acc", s.group_acc),
            ] {
                close(got, key);
            }
            for (key, got) in [
                ("violations", s.violations.len()),
                ("missing", s.missing.len()),
                ("extra", s.extra.len()),
            ] {
                assert_eq!(
                    got as u64,
                    want[key].as_u64().unwrap(),
                    "{id} {key}: {:?}",
                    (&s.violations, &s.missing, &s.extra)
                );
            }
        }
    }
}
