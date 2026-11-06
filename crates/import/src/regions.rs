//! An imported page as a board, ported from src/engine/regions.ts. The page's size is kept on the
//! board, and each block of content becomes a node fixed in place.

use boardcast_board::graph::{Graph, Kind, Node, NodeData, Ordered};
use boardcast_board::spans::RegionRow;
use serde_json::{json, Map};
use std::collections::HashMap;

/// The longest label: the model reads every label, and the start of a block is enough to point at it.
const MAX: usize = 36;

/// A block of content on a page, in board coordinates with the page's top left as the origin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Area {
    pub text: String,
    /// A given name such as "Figure 3: ..."; otherwise the label is the start of the text.
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// The text layer, which lets a word or sentence be marked.
    pub rows: Option<Vec<RegionRow>>,
}

pub(crate) fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

pub(crate) fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn utf16_prefix(s: &str, units: usize) -> String {
    let kept: Vec<u16> = s.encode_utf16().take(units).collect();
    String::from_utf16_lossy(&kept)
}

pub fn page_graph(w: f64, h: f64, number: Option<u32>, areas: &[Area]) -> Graph {
    let mut g = Graph::default();
    let mut size = json!({ "w": js_round(w) as i64, "h": js_round(h) as i64 });
    if let Some(number) = number {
        size["no"] = json!(number);
    }
    g.extra.insert("underlay".into(), size);
    let mut used: HashMap<String, u32> = HashMap::new();
    for a in areas {
        let source = a.name.as_deref().unwrap_or(&a.text).replace(['「', '」'], "");
        let flat = source.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.is_empty() {
            continue;
        }
        let long = utf16_len(&flat) > MAX;
        let mut label = if long { format!("{}…", utf16_prefix(&flat, MAX - 1)) } else { flat.clone() };
        // Two blocks that start alike, such as two table rows, must stay two.
        let n = used.entry(label.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            label = format!("{label}（{n}）");
        }
        let mut extra = Map::new();
        extra.insert("at".into(), json!({ "x": js_round(a.x) as i64, "y": js_round(a.y) as i64, "w": js_round(a.w) as i64, "h": js_round(a.h) as i64 }));
        if long {
            extra.insert("text".into(), json!(flat));
        }
        if let Some(rows) = a.rows.as_ref().filter(|r| !r.is_empty()) {
            extra.insert("rows".into(), json!(rows));
        }
        let id = format!("n{}", g.next_node);
        g.next_node += 1;
        let node: Node = Ordered::new(NodeData { id, label, aliases: vec![], kind: Kind::Region, parent: None, group: None, marks: vec![], icon: None, ink: "ink".into(), extra });
        g.nodes.push(node);
    }
    g
}
