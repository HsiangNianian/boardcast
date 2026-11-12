//! Cutting a meeting into passages that are each about one thing and short enough to embed well.

use boardcast_board::graph::{Graph, Kind};
use serde::{Deserialize, Serialize};

/// Where a passage came from, so a hit can open the meeting at that place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    /// Transcript lines `first..=last`, by their index in the meeting.
    Transcript { first: usize, last: usize },
    /// A node of a board, with what hangs under it.
    Board { page: usize, node: String },
    /// A part of the summary, under its heading.
    Summary { heading: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub source: Source,
    pub text: String,
}

/// A line of the transcript as said: who, if known, and what.
pub struct Said<'a> {
    pub speaker: Option<&'a str>,
    pub text: &'a str,
}

/// A passage of transcript grows to about this many characters, or this many lines.
const PASSAGE_CHARS: usize = 360;
const PASSAGE_LINES: usize = 6;

/// Runs of consecutive lines, each sharing its first line with the end of the one before, so a thought that
/// crosses the cut is whole in one of them. Lines with nothing to say ("嗯", "Okay.") ride along but never
/// start a passage of their own.
pub fn transcript(lines: &[Said]) -> Vec<Chunk> {
    let worth = |s: &Said| s.text.trim().chars().filter(|c| c.is_alphanumeric()).count() >= 4;
    let mut out = vec![];
    let mut start = 0;
    while start < lines.len() {
        while start < lines.len() && !worth(&lines[start]) {
            start += 1;
        }
        if start >= lines.len() {
            break;
        }
        let (mut end, mut chars) = (start, 0);
        while end < lines.len() && (end == start || (chars < PASSAGE_CHARS && end - start < PASSAGE_LINES)) {
            chars += lines[end].text.chars().count();
            end += 1;
        }
        let text = lines[start..end]
            .iter()
            .map(|l| match l.speaker.filter(|s| !s.is_empty()) {
                Some(who) => format!("{who}: {}", l.text.trim()),
                None => l.text.trim().to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push(Chunk { source: Source::Transcript { first: start, last: end - 1 }, text });
        if end >= lines.len() {
            break;
        }
        // Overlap by one line, unless the passage was a single long line.
        start = if end - start > 1 { end - 1 } else { end };
    }
    out
}

/// Each topic with the items under it, and each node that stands alone: what it says, where it leads.
pub fn board(page: usize, graph: &Graph) -> Vec<Chunk> {
    let label = |id: &str| graph.nodes.iter().find(|n| n.id == id).map(|n| n.label.clone()).unwrap_or_default();
    let children = |id: &str| graph.nodes.iter().filter(move |n| n.parent.as_deref() == Some(id)).collect::<Vec<_>>();
    let mut out = vec![];
    for node in graph.nodes.iter().filter(|n| n.parent.is_none()) {
        // An imported page's block keeps its whole text apart from its short label: that is what to search.
        if matches!(node.kind, Kind::Region) {
            let full = node.extra.get("text").and_then(|t| t.as_str()).filter(|t| !t.trim().is_empty()).unwrap_or(&node.label);
            for piece in pieces(full) {
                out.push(Chunk { source: Source::Board { page, node: node.id.clone() }, text: piece });
            }
            continue;
        }
        let mut text = node.label.clone();
        if matches!(node.kind, Kind::Topic) {
            // Two levels are plenty to say what a topic holds.
            for child in children(&node.id) {
                text.push_str(&format!("\n- {}", child.label));
                for grandchild in children(&child.id) {
                    text.push_str(&format!("\n  - {}", grandchild.label));
                }
            }
        }
        for edge in graph.edges.iter().filter(|e| e.from == node.id) {
            match &edge.label {
                Some(l) if !l.is_empty() => text.push_str(&format!("\n→ {l} → {}", label(&edge.to))),
                _ => text.push_str(&format!("\n→ {}", label(&edge.to))),
            }
        }
        if text.trim().is_empty() {
            continue;
        }
        out.push(Chunk { source: Source::Board { page, node: node.id.clone() }, text });
    }
    out
}

/// A long text in passages of about `PASSAGE_CHARS`, cut after a sentence where one ends near enough.
fn pieces(text: &str) -> Vec<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + PASSAGE_CHARS).min(chars.len());
        if end < chars.len() {
            // Back to the last sentence end in the second half of the passage, if there is one.
            let half = start + PASSAGE_CHARS / 2;
            if let Some(cut) = (half..end).rev().find(|&i| matches!(chars[i], '.' | '?' | '!' | '。' | '？' | '！' | '；' | ';')) {
                end = cut + 1;
            }
        }
        let piece: String = chars[start..end].iter().collect::<String>().trim().to_string();
        if !piece.is_empty() {
            out.push(piece);
        }
        start = end;
    }
    out
}

/// The summary, one passage per section.
pub fn summary(markdown: &str) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = vec![];
    let mut heading = String::new();
    let mut body = String::new();
    let flush = |heading: &str, body: &mut String, out: &mut Vec<Chunk>| {
        if !body.trim().is_empty() {
            let text = if heading.is_empty() { body.trim().to_string() } else { format!("{heading}\n{}", body.trim()) };
            out.push(Chunk { source: Source::Summary { heading: heading.to_string() }, text });
        }
        body.clear();
    };
    for line in markdown.lines() {
        if let Some(h) = line.strip_prefix("### ").or_else(|| line.strip_prefix("## ")) {
            flush(&heading, &mut body, &mut out);
            heading = h.trim().to_string();
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(&heading, &mut body, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said<'a>(lines: &[&'a str]) -> Vec<Said<'a>> {
        lines.iter().map(|t| Said { speaker: Some("A"), text: t }).collect()
    }

    #[test]
    fn passages_overlap_by_a_line_and_skip_fillers_at_the_start() {
        let long = "x".repeat(200);
        let lines = said(&["嗯", &long, &long, &long, "Okay."]);
        let chunks = transcript(&lines);
        let ranges: Vec<_> = chunks.iter().map(|c| c.source.clone()).collect();
        assert_eq!(ranges[0], Source::Transcript { first: 1, last: 2 });
        assert_eq!(ranges[1], Source::Transcript { first: 2, last: 3 });
        assert!(chunks.iter().all(|c| !c.text.starts_with("A: 嗯")));
    }

    #[test]
    fn a_summary_is_cut_at_its_headings() {
        let parts = summary("### Conclusions\n- ship tuesday\n\n### To-dos\n- [ ] canary");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].source, Source::Summary { heading: "To-dos".into() });
        assert!(parts[1].text.contains("canary"));
    }

    #[test]
    fn an_imported_block_is_searched_by_its_whole_text_in_passages() {
        let mut g = Graph::default();
        let body = "Deep networks are hard to train. ".repeat(30);
        let mut extra = serde_json::Map::new();
        extra.insert("text".into(), serde_json::json!(body));
        g.nodes.push(boardcast_board::graph::Ordered::new(boardcast_board::graph::NodeData {
            id: "n1".into(), label: "Deep networks are…".into(), aliases: vec![], kind: Kind::Region, parent: None, group: None, marks: vec![], icon: None, ink: "ink".into(), extra,
        }));
        let parts = board(2, &g);
        assert!(parts.len() >= 3, "{} passages", parts.len());
        assert!(parts.iter().all(|c| c.text.chars().count() <= PASSAGE_CHARS && c.text.ends_with('.')));
        assert!(parts.iter().all(|c| c.source == Source::Board { page: 2, node: "n1".into() }));
    }

    #[test]
    fn a_topic_carries_its_items() {
        let mut g = Graph::default();
        g.apply(&boardcast_board::graph::parse_ops("T 「Release」\nL 「Release」 「Load testing」\nE 「API」 -> 「USER」 : 「auth」"));
        let parts = board(0, &g);
        assert!(parts.iter().any(|c| c.text.contains("Release") && c.text.contains("- Load testing")));
        assert!(parts.iter().any(|c| c.text.contains("API") && c.text.contains("→ auth → USER")));
    }
}
