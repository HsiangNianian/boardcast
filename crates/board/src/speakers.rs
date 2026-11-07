//! Who said what, as in src/engine/voices.ts and attribute.ts. Each sentence's voiceprint is compared
//! with the voices this meeting has heard, and what a model puts on the board remembers who raised it.

use crate::graph::{norm, Edge, Graph, Node, Utterance};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// How alike two voiceprints must be to count as one person. CAM++ usually gives one person above 0.5
/// and two people below 0.2; sentences in a meeting are short, so this is a little looser.
const SAME: f64 = 0.45;
/// The most sentences a voice is weighted by, as a voice changes and old sentences must not outweigh new ones.
const MEMORY: u32 = 30;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Voice {
    pub name: String,
    /// The average direction of this person's voiceprints, at unit length.
    pub centroid: Vec<f64>,
    /// How many sentences went into it.
    pub n: u32,
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Unit length, with four decimals being enough in a meeting file.
fn unit(v: &[f64]) -> Vec<f64> {
    let len = dot(v, v).sqrt();
    let len = if len == 0.0 { 1.0 } else { len };
    v.iter().map(|x| (x / len * 1e4 + 0.5).floor() / 1e4).collect()
}

/// The next unused name: A to Z, then A2, B2 and so on.
pub fn next_name<'a>(taken: impl IntoIterator<Item = &'a str>) -> String {
    let used: BTreeSet<&str> = taken.into_iter().collect();
    (1..)
        .flat_map(|round: u32| ('A'..='Z').map(move |c| if round > 1 { format!("{c}{round}") } else { c.to_string() }))
        .find(|name| !used.contains(name.as_str()))
        .expect("names never run out")
}

/// Say whose voice this is, joining it to that voice or adding a new one. `taken` holds names already
/// in use in the meeting, including hand-written ones, which a new voice must not share.
pub fn identify(voices: &mut Vec<Voice>, voiceprint: &[f64], taken: &[&str]) -> String {
    // The first of equals wins, as in the older app.
    let mut closest: Option<(usize, f64)> = None;
    for (i, voice) in voices.iter().enumerate() {
        let score = dot(&voice.centroid, voiceprint);
        if closest.is_none_or(|(_, best)| score > best) {
            closest = Some((i, score));
        }
    }
    if let Some((i, _)) = closest.filter(|(_, score)| *score >= SAME) {
        let voice = &mut voices[i];
        let weight = f64::from(voice.n.min(MEMORY));
        let merged: Vec<f64> = voice.centroid.iter().enumerate().map(|(k, x)| x * weight + voiceprint.get(k).copied().unwrap_or(0.0)).collect();
        voice.centroid = unit(&merged);
        voice.n += 1;
        return voice.name.clone();
    }
    let name = next_name(taken.iter().copied().chain(voices.iter().map(|v| v.name.as_str())));
    voices.push(Voice { name: name.clone(), centroid: unit(voiceprint), n: 1 });
    name
}

/// Rename a voice. Taking a name another voice has joins the two, which had been told apart by mistake.
pub fn rename_voice(voices: &mut Vec<Voice>, from: &str, to: &str) {
    if from == to {
        return;
    }
    let Some(a) = voices.iter().position(|v| v.name == from) else { return };
    let Some(b) = voices.iter().position(|v| v.name == to) else {
        voices[a].name = to.to_string();
        return;
    };
    let (one, other) = (voices[a].clone(), voices[b].clone());
    let sum: Vec<f64> = other.centroid.iter().enumerate().map(|(k, x)| x * f64::from(other.n) + one.centroid.get(k).copied().unwrap_or(0.0) * f64::from(one.n)).collect();
    voices.retain(|v| v.name != from && v.name != to);
    voices.push(Voice { name: to.to_string(), centroid: unit(&sum), n: one.n + other.n });
}

/// The letter shown for a speaker: a Chinese surname (the second character after 小, 老 or 阿), a
/// trailing number, or else the first letter.
pub fn initial(name: &str) -> String {
    let name = name.trim();
    let digits = name.chars().rev().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        return name[name.len() - digits.min(2)..].to_string();
    }
    let chars: Vec<char> = name.chars().collect();
    if chars.first().copied().is_some_and(is_han) {
        return (if chars.len() > 1 && "小老阿".contains(chars[0]) { chars[1] } else { chars[0] }).to_string();
    }
    chars.iter().find(|c| c.is_alphanumeric()).map_or("?".to_string(), |c| c.to_uppercase().collect())
}

fn is_han(c: char) -> bool {
    matches!(c, '\u{3005}' | '\u{3007}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{323AF}')
}

/// Pieces of two characters, or the one character there is.
fn grams(text: &str) -> BTreeSet<String> {
    let chars: Vec<char> = norm(text).chars().collect();
    if chars.len() == 1 {
        return BTreeSet::from([chars[0].to_string()]);
    }
    chars.windows(2).map(|pair| pair.iter().collect()).collect()
}

/// How many pieces of these names appear in a sentence.
fn overlap(text: &str, names: &[&str]) -> usize {
    let text = norm(text);
    names.iter().flat_map(|name| grams(name)).filter(|gram| text.contains(gram.as_str())).count()
}

fn speaker_of(line: &Utterance) -> Option<&str> {
    line.speaker.as_deref().filter(|s| !s.is_empty())
}

/// Who in this batch most likely said these names. With one speaker it is them, among equals the
/// earliest, and with no match or no speakers nobody knows.
fn who_said<'a>(batch: &'a [Utterance], names: &[&str]) -> Option<&'a str> {
    let speakers: BTreeSet<&str> = batch.iter().filter_map(speaker_of).collect();
    if speakers.len() <= 1 {
        return speakers.first().copied();
    }
    let mut best = None;
    let mut top = 0;
    for line in batch {
        let Some(speaker) = speaker_of(line) else { continue };
        let score = overlap(&line.text, names);
        if score > top {
            top = score;
            best = Some(speaker);
        }
    }
    best
}

pub fn by_of_node(node: &Node) -> Option<&str> {
    node.extra.get("by").and_then(Value::as_str)
}

pub fn by_of_edge(edge: &Edge) -> Option<&str> {
    edge.extra.get("by").and_then(Value::as_str)
}

fn is_set(by: Option<&str>) -> bool {
    by.is_some_and(|s| !s.is_empty())
}

/// What a model newly inked this turn remembers who raised it. What was there already stays as it was,
/// even if someone later renamed, marked or moved it.
pub fn attribute(before: &Graph, after: &mut Graph, batch: &[Utterance]) {
    let inked: BTreeSet<&str> = before.nodes.iter().filter(|n| n.ink == "ink").map(|n| n.id.as_str()).collect();
    let inked_edges: BTreeSet<(&str, &str)> = before.edges.iter().filter(|e| e.ink == "ink").map(|e| (e.from.as_str(), e.to.as_str())).collect();
    let mut raised = vec![];
    for (i, node) in after.nodes.iter().enumerate() {
        if node.ink == "ink" && !is_set(by_of_node(node)) && !inked.contains(node.id.as_str()) {
            let mut names = vec![node.label.as_str()];
            names.extend(node.aliases.iter().map(String::as_str));
            raised.push((i, who_said(batch, &names).map(str::to_string)));
        }
    }
    for (i, by) in raised {
        if let Some(by) = by {
            after.nodes[i].extra.insert("by".into(), Value::from(by));
        }
    }
    let label = |id: &str| after.nodes.iter().find(|n| n.id == id).map_or(String::new(), |n| n.label.clone());
    let mut raised = vec![];
    for (i, edge) in after.edges.iter().enumerate() {
        if edge.ink == "ink" && !is_set(by_of_edge(edge)) && !inked_edges.contains(&(edge.from.as_str(), edge.to.as_str())) {
            let (from, to) = (label(&edge.from), label(&edge.to));
            raised.push((i, who_said(batch, &[&from, &to, edge.label.as_deref().unwrap_or_default()]).map(str::to_string)));
        }
    }
    for (i, by) in raised {
        if let Some(by) = by {
            after.edges[i].extra.insert("by".into(), Value::from(by));
        }
    }
}

/// A speaker's new name, or two speakers becoming one, on the board.
pub fn rename_by(graph: &mut Graph, from: &str, to: &str) {
    let fix = |extra: &mut serde_json::Map<String, Value>| {
        if extra.get("by").and_then(Value::as_str) == Some(from) {
            extra.insert("by".into(), Value::from(to));
        }
    };
    graph.nodes.iter_mut().for_each(|n| fix(&mut n.extra));
    graph.edges.iter_mut().for_each(|e| fix(&mut e.extra));
}

/// The same in a replay record, which stores nodes and edges as written.
pub fn rename_by_in_steps(steps: &mut [Value], from: &str, to: &str) {
    for step in steps {
        for part in ["nodes", "edges"] {
            let Some(set) = step.pointer_mut(&format!("/d/{part}/set")).and_then(Value::as_array_mut) else { continue };
            for item in set {
                if item.get("by").and_then(Value::as_str) == Some(from) {
                    item["by"] = Value::from(to);
                }
            }
        }
    }
}

/// How many transcript lines there were when this node first appeared in a replay record.
pub fn first_seen(steps: &[Value], node: &str) -> Option<usize> {
    let has = |step: &&Value| step.pointer("/d/nodes/set").and_then(Value::as_array).is_some_and(|set| set.iter().any(|n| n["id"] == node));
    let step = steps.iter().find(has)?;
    step["line"].as_u64().map(|line| line as usize)
}

/// Which line of the transcript this came from: what `who` said on this page most like its names,
/// before `before` lines if that is known. Among equals the latest, which is what put it on the board.
pub fn said_at(lines: &[Value], who: &str, names: &[&str], page: usize, before: Option<usize>) -> Option<usize> {
    let mut best = None;
    let mut top = 0;
    for (i, line) in lines.iter().enumerate().take(before.unwrap_or(lines.len())) {
        if line["manual"].as_bool().unwrap_or(false) || line["speaker"] != who || line["page"].as_u64().unwrap_or(0) as usize != page {
            continue;
        }
        let score = overlap(line["text"].as_str().unwrap_or_default(), names);
        if best.is_none() || score >= top {
            top = score;
            best = Some(i);
        }
    }
    best
}

/// How many things on this board a speaker raised.
pub fn raised_by(graph: &Graph, who: &str) -> usize {
    graph.nodes.iter().filter(|n| by_of_node(n) == Some(who)).count() + graph.edges.iter().filter(|e| by_of_edge(e) == Some(who)).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn goldens() -> Value {
        serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap()
    }

    fn strings(value: &Value) -> Vec<String> {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn voices_are_told_apart_as_the_typescript_engine_does() {
        let g = goldens();
        for case in g["voices"].as_array().unwrap() {
            let mut voices: Vec<Voice> = vec![];
            for step in case["steps"].as_array().unwrap() {
                let print: Vec<f64> = serde_json::from_value(step["print"].clone()).unwrap();
                let taken = strings(&step["taken"]);
                let taken: Vec<&str> = taken.iter().map(String::as_str).collect();
                let name = identify(&mut voices, &print, &taken);
                assert_eq!(name, step["name"].as_str().unwrap(), "{}", case["name"]);
                let wanted: Vec<Voice> = serde_json::from_value(step["voices"].clone()).unwrap();
                assert_eq!(voices, wanted, "{}", case["name"]);
            }
            for rename in case["renames"].as_array().unwrap() {
                rename_voice(&mut voices, rename["from"].as_str().unwrap(), rename["to"].as_str().unwrap());
                let wanted: Vec<Voice> = serde_json::from_value(rename["voices"].clone()).unwrap();
                assert_eq!(voices, wanted, "{}", case["name"]);
            }
        }
    }

    #[test]
    fn names_and_initials_are_as_in_the_typescript_engine() {
        let g = goldens();
        for case in g["names"].as_array().unwrap() {
            let taken = strings(&case["taken"]);
            assert_eq!(next_name(taken.iter().map(String::as_str)), case["next"].as_str().unwrap());
        }
        for case in g["initials"].as_array().unwrap() {
            assert_eq!(initial(case["name"].as_str().unwrap()), case["initial"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn what_is_raised_is_attributed_as_the_typescript_engine_does() {
        let g = goldens();
        for case in g["attribution"].as_array().unwrap() {
            let before: Graph = serde_json::from_value(case["before"].clone()).unwrap();
            let mut after: Graph = serde_json::from_value(case["after"].clone()).unwrap();
            let batch: Vec<Utterance> = serde_json::from_value(case["batch"].clone()).unwrap();
            attribute(&before, &mut after, &batch);
            assert_eq!(serde_json::to_value(&after).unwrap(), case["attributed"], "{}", case["name"]);
            rename_by(&mut after, "A", "甲");
            assert_eq!(serde_json::to_value(&after).unwrap(), case["renamed"], "{}", case["name"]);
        }
    }

    #[test]
    fn a_line_is_found_as_the_typescript_engine_finds_it() {
        let g = goldens();
        for case in g["sources"].as_array().unwrap() {
            let (lines, steps) = (case["lines"].as_array().unwrap(), case["steps"].as_array().unwrap());
            let seen = first_seen(steps, case["node"].as_str().unwrap());
            assert_eq!(seen, case["seen"].as_u64().map(|n| n as usize), "{}", case["name"]);
            let names = strings(&case["names"]);
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            let found = said_at(lines, case["who"].as_str().unwrap(), &names, case["page"].as_u64().unwrap() as usize, seen);
            assert_eq!(found, case["found"].as_u64().map(|n| n as usize), "{}", case["name"]);
        }
    }

    #[test]
    fn a_replay_record_takes_the_new_name_too() {
        let mut steps = vec![json!({ "t": 1, "line": 1, "d": { "nodes": { "set": [{ "id": "n1", "by": "A" }, { "id": "n2", "by": "B" }] }, "edges": { "set": [{ "from": "n1", "to": "n2", "by": "A" }] } } }), json!({ "t": 2, "d": {} })];
        rename_by_in_steps(&mut steps, "A", "甲");
        assert_eq!(steps[0]["d"]["nodes"]["set"][0]["by"], "甲");
        assert_eq!(steps[0]["d"]["nodes"]["set"][1]["by"], "B");
        assert_eq!(steps[0]["d"]["edges"]["set"][0]["by"], "甲");
    }
}
