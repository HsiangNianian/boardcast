//! The board's state and the model's operations, ported from src/engine/{graph,ops,prompt,lang}.ts.
//! Pencil drafts are left out, as free editing is gone. Tests hold this to the TypeScript engine's output on real recorded model replies; see fixtures/make-goldens.mjs.

use crate::spans::{locate, RegionRow, Span, Unit};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};

const MARKS: [&str; 5] = ["划掉", "圈出", "下划线", "星标", "高亮"];
/// The most of an imported block's text the model is shown.
const REGION_TEXT: usize = 3000;
const ICONS: [&str; 9] = ["数据库", "表格", "手机", "人", "服务器", "文档", "云", "图表", "消息"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Node,
    Topic,
    Item,
    Question,
    Formula,
    Note,
    /// A block of an imported page, fixed in place. The page image shows it, so only marks are drawn.
    Region,
}

impl Kind {
    fn tag(self) -> &'static str {
        match self {
            Self::Node | Self::Item => "",
            Self::Topic => "[话题] ",
            Self::Question => "[问题] ",
            Self::Formula => "[公式] ",
            Self::Note => "[备注] ",
            Self::Region => "[原文] ",
        }
    }
}

fn ink() -> String {
    "ink".into()
}

fn one() -> u32 {
    1
}

// Meeting files are shared with the older app. `extra` carries every field this code does not
// model (speaker attribution, imported text, future additions), so a save never loses them.
/// A value that remembers the order of its JSON fields. The older app compares nodes by their exact
/// text, so a node written back with its fields rearranged would look edited and break its replay.
#[derive(Clone, Debug)]
pub struct Ordered<T> {
    value: T,
    order: Vec<String>,
}

impl<T> Ordered<T> {
    pub fn new(value: T) -> Self {
        Self { value, order: vec![] }
    }
}

impl<T> Deref for Ordered<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Ordered<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Field order is how the value is written, not what it is.
impl<T: PartialEq> PartialEq for Ordered<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Serialize> Serialize for Ordered<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Value::Object(mut fields) = serde_json::to_value(&self.value).map_err(serde::ser::Error::custom)? else {
            return Err(serde::ser::Error::custom("expected an object"));
        };
        let mut out = Map::new();
        for key in &self.order {
            if let Some(v) = fields.shift_remove(key) {
                out.insert(key.clone(), v);
            }
        }
        // Fields that are new to this value go last.
        out.extend(fields);
        out.serialize(serializer)
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Ordered<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = Map::<String, Value>::deserialize(deserializer)?;
        let order = fields.keys().cloned().collect();
        let value = serde_json::from_value(Value::Object(fields)).map_err(serde::de::Error::custom)?;
        Ok(Self { value, order })
    }
}

pub type Node = Ordered<NodeData>;
pub type Edge = Ordered<EdgeData>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeData {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default)]
    pub marks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default = "ink")]
    pub ink: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeData {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default = "ink")]
    pub ink: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(rename = "nextNode", default = "one")]
    pub next_node: u32,
    #[serde(rename = "nextGroup", default = "one")]
    pub next_group: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Graph {
    fn default() -> Self {
        Self { nodes: vec![], edges: vec![], groups: vec![], next_node: 1, next_group: 1, extra: Map::new() }
    }
}

/// Names compare without case, spaces or common punctuation.
pub fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !"·・,，。.、\"“”'‘’「」『』()（）?？!！".contains(*c))
        .collect()
}

impl Graph {
    fn find(&self, name: &str) -> Option<usize> {
        let key = norm(name);
        if key.is_empty() {
            return None;
        }
        self.nodes
            .iter()
            .position(|n| norm(&n.label) == key || n.aliases.iter().any(|a| norm(a) == key))
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    fn add(&mut self, label: &str, kind: Kind, parent: Option<String>) -> String {
        let id = format!("n{}", self.next_node);
        self.next_node += 1;
        self.nodes.push(Ordered::new(NodeData {
            id: id.clone(),
            label: label.to_string(),
            aliases: vec![],
            kind,
            parent,
            group: None,
            marks: vec![],
            icon: None,
            ink: ink(),
            extra: Map::new(),
        }));
        id
    }

    /// Reference -> node id. `create` is the kind a new name becomes.
    fn resolve(&mut self, r: &Ref, create: Option<Kind>) -> Option<String> {
        let name = match r {
            Ref::Id(id) => {
                if self.index(id).is_some() {
                    return Some(id.clone());
                }
                // Looks like an id but does not exist: never create a node called "n12".
                if looks_like_id(id) {
                    return None;
                }
                id
            }
            Ref::Name(name) => name,
        };
        if let Some(i) = self.find(name) {
            return Some(self.nodes[i].id.clone());
        }
        let kind = create?;
        if norm(name).is_empty() {
            return None;
        }
        Some(self.add(name, kind, None))
    }

    /// A hand-made mark from the board's toolbar: on if absent, off if present.
    pub fn toggle_mark(&mut self, id: &str, mark: &str) {
        let Some(i) = self.index(id) else { return };
        let marks = &mut self.nodes[i].marks;
        if let Some(at) = marks.iter().position(|m| m == mark) {
            marks.remove(at);
        } else if MARKS.contains(&mark) {
            marks.push(mark.to_string());
        }
    }

    /// The node and all its descendants.
    fn subtree(&self, id: &str) -> HashSet<String> {
        let mut out = HashSet::from([id.to_string()]);
        loop {
            let before = out.len();
            for n in &self.nodes {
                if n.parent.as_ref().is_some_and(|p| out.contains(p)) {
                    out.insert(n.id.clone());
                }
            }
            if out.len() == before {
                return out;
            }
        }
    }

    /// The size of the imported page under this board, if there is one.
    pub fn underlay(&self) -> Option<(f64, f64)> {
        let u = self.extra.get("underlay")?;
        Some((u["w"].as_f64()?, u["h"].as_f64()?))
    }

    /// Mark a word or sentence by hand, or take the mark off if it is already there.
    pub fn toggle_span(&mut self, id: &str, span: Span) {
        let Some(i) = self.index(id) else { return };
        let node = &mut self.nodes[i];
        let spans = node.spans();
        if spans.iter().any(|s| s.same(&span)) {
            let left: Vec<Span> = spans.into_iter().filter(|s| !s.same(&span)).collect();
            if left.is_empty() {
                node.extra.remove("spans");
            } else {
                node.extra.insert("spans".into(), json!(left));
            }
        } else {
            node.add_span(span);
        }
    }

    /// Mark a quote on the block it is in, wherever the model pointed: it may pick the wrong block.
    fn mark_quote(&mut self, target: usize, quote: &str, sentence: bool, mark: &str) -> bool {
        let unit = if sentence { Unit::Sentence } else { Unit::Word };
        let others = (0..self.nodes.len()).filter(|i| *i != target && self.nodes[*i].kind == Kind::Region);
        let hosts: Vec<usize> = std::iter::once(target).chain(others).collect();
        for host in hosts {
            let rows = self.nodes[host].rows();
            if rows.is_empty() {
                continue;
            }
            if let Some(span) = locate(&rows, quote, mark, unit) {
                self.nodes[host].add_span(span);
                self.nodes[host].ink = "ink".to_string();
                return true;
            }
        }
        false
    }

    /// Put imported text back as it was, keeping any marks made on it.
    fn restore_pinned(&mut self, pinned: Vec<Node>) {
        let ids: Vec<String> = pinned.iter().map(|n| n.id.clone()).collect();
        for was in pinned {
            match self.index(&was.id) {
                None => self.nodes.push(was),
                Some(i) => {
                    let n = &mut self.nodes[i];
                    n.label.clone_from(&was.label);
                    n.aliases.clone_from(&was.aliases);
                    n.kind = Kind::Region;
                    n.parent = None;
                    n.group = None;
                    match was.extra.get("at") {
                        Some(at) => {
                            n.extra.insert("at".into(), at.clone());
                        }
                        None => {
                            n.extra.remove("at");
                        }
                    }
                }
            }
        }
        // Only notes and the like can hang on imported text.
        for n in &mut self.nodes {
            if n.kind == Kind::Item && n.parent.as_ref().is_some_and(|p| ids.contains(p)) {
                n.kind = Kind::Note;
            }
        }
    }

    pub fn children<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Node> {
        self.nodes.iter().filter(move |n| n.parent.as_deref() == Some(id))
    }

    pub fn describe(&self) -> String {
        if self.nodes.is_empty() && self.groups.is_empty() {
            return "（空）".into();
        }
        let mut lines: Vec<String> = self.groups.iter().map(|x| format!("{} [组] {}", x.id, x.label)).collect();
        for n in &self.nodes {
            let attached = n.parent.as_deref().is_some_and(|p| self.index(p).is_some());
            if !attached {
                self.describe_node(n, 0, &mut lines);
            }
        }
        let name = |id: &str| self.index(id).map_or("", |i| self.nodes[i].label.as_str());
        for e in &self.edges {
            let label = e.label.as_ref().filter(|l| !l.is_empty()).map_or(String::new(), |l| format!(" {l}"));
            lines.push(format!("{}({}) -> {}({}){label}", e.from, name(&e.from), e.to, name(&e.to)));
        }
        lines.join("\n")
    }

    fn describe_node(&self, n: &Node, depth: usize, lines: &mut Vec<String>) {
        let mut extra = vec![];
        if !n.aliases.is_empty() {
            extra.push(format!("又称 {}", n.aliases.join("/")));
        }
        if let Some(g) = &n.group {
            extra.push(format!("在 {g}"));
        }
        if !n.marks.is_empty() {
            extra.push(n.marks.join("、"));
        }
        for s in n.spans() {
            extra.push(format!("{}「{}」", s.mark, utf16_prefix(&s.text, 20)));
        }
        if let Some(icon) = &n.icon {
            extra.push(format!("图标 {icon}"));
        }
        let extra = if extra.is_empty() { String::new() } else { format!("（{}）", extra.join("，")) };
        let shown = match n.extra.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            Some(text) if text.encode_utf16().count() > REGION_TEXT => format!("{}…", utf16_prefix(text, REGION_TEXT)),
            Some(text) => text.to_string(),
            None => n.label.clone(),
        };
        lines.push(format!("{}{} {}{shown}{extra}", "  ".repeat(depth), n.id, n.kind.tag()));
        for c in self.children(&n.id) {
            self.describe_node(c, depth + 1, lines);
        }
    }

    /// Apply operations in order. Unresolvable ones are skipped, as a bad line must not cost a batch.
    pub fn apply(&mut self, ops: &[Op]) {
        // Imported text stays put: the model can mark it, link it and note on it, but not change it.
        let pinned: Vec<Node> = self.nodes.iter().filter(|n| n.kind == Kind::Region).cloned().collect();
        for o in ops {
            match o {
                Op::N(name) => {
                    self.resolve(&Ref::Name(name.clone()), Some(Kind::Node));
                }
                Op::T(name) => {
                    if let Some(id) = self.resolve(&Ref::Name(name.clone()), Some(Kind::Topic)) {
                        let i = self.index(&id).expect("resolved");
                        if self.nodes[i].kind == Kind::Node {
                            self.nodes[i].kind = Kind::Topic;
                        }
                    }
                }
                Op::L(parent, name) => {
                    let Some(parent) = self.resolve(parent, Some(Kind::Topic)) else { continue };
                    match self.find(name) {
                        Some(i) if self.nodes[i].id == parent => {}
                        Some(i) => {
                            let id = self.nodes[i].id.clone();
                            // Already on the board: moving it under the new parent is the request.
                            if !self.subtree(&id).contains(&parent) {
                                let n = &mut self.nodes[i];
                                n.parent = Some(parent);
                                if matches!(n.kind, Kind::Node | Kind::Topic) {
                                    n.kind = Kind::Item;
                                }
                            }
                        }
                        None => {
                            self.add(name, Kind::Item, Some(parent));
                        }
                    }
                }
                Op::Attached(kind, parent, text) => {
                    let parent = parent.as_ref().and_then(|p| self.resolve(p, None));
                    if self.find(text).is_none() {
                        self.add(text, *kind, parent);
                    }
                }
                Op::K(target, text) => {
                    if let Some(target) = self.resolve(target, None) {
                        if self.find(text).is_none() {
                            self.add(text, Kind::Note, Some(target));
                        }
                    }
                }
                Op::M(target, mark, quote) => {
                    if let Some(i) = self.resolve(target, None).and_then(|id| self.index(&id)) {
                        if let Some((text, sentence)) = quote.as_ref().filter(|_| self.nodes[i].kind == Kind::Region) {
                            if self.mark_quote(i, text, *sentence, mark) {
                                continue;
                            }
                        }
                        if !self.nodes[i].marks.contains(mark) {
                            self.nodes[i].marks.push(mark.clone());
                        }
                    }
                }
                Op::H(_) => {}
                Op::I(target, icon) => {
                    if let Some(i) = self.resolve(target, None).and_then(|id| self.index(&id)) {
                        self.nodes[i].icon = Some(icon.clone());
                    }
                }
                Op::E(from, to, label) => {
                    let from = self.resolve(from, Some(Kind::Node));
                    let to = self.resolve(to, Some(Kind::Node));
                    let (Some(from), Some(to)) = (from, to) else { continue };
                    if from == to {
                        continue;
                    }
                    if let Some(old) = self.edges.iter_mut().find(|e| e.from == from && e.to == to) {
                        if label.is_some() {
                            old.label.clone_from(label);
                        }
                    } else {
                        self.edges.push(Ordered::new(EdgeData { from, to, label: label.clone(), ink: ink(), extra: Map::new() }));
                    }
                }
                Op::X(from, to) => {
                    let from = self.resolve(from, None);
                    let to = self.resolve(to, None);
                    self.edges.retain(|e| !(from.as_ref() == Some(&e.from) && to.as_ref() == Some(&e.to)));
                }
                Op::R(target, name) => {
                    let Some(i) = self.resolve(target, None).and_then(|id| self.index(&id)) else { continue };
                    let n = &mut self.nodes[i];
                    if norm(&n.label) == norm(name) {
                        continue;
                    }
                    if !n.aliases.iter().any(|a| norm(a) == norm(&n.label)) {
                        let old = n.label.clone();
                        n.aliases.push(old);
                    }
                    n.label.clone_from(name);
                    n.aliases.retain(|a| norm(a) != norm(name));
                }
                Op::A(target, alias) => {
                    let Some(i) = self.resolve(target, None).and_then(|id| self.index(&id)) else { continue };
                    if self.find(alias).is_none() {
                        self.nodes[i].aliases.push(alias.clone());
                    }
                }
                Op::G(group, members) => {
                    let existing = self.groups.iter().find(|x| norm(&x.label) == norm(group)).map(|x| x.id.clone());
                    let id = existing.unwrap_or_else(|| {
                        let id = format!("g{}", self.next_group);
                        self.next_group += 1;
                        self.groups.push(Group { id: id.clone(), label: group.clone() });
                        id
                    });
                    for m in members {
                        if let Some(i) = self.resolve(m, Some(Kind::Node)).and_then(|n| self.index(&n)) {
                            self.nodes[i].group = Some(id.clone());
                        }
                    }
                }
                Op::D(target) => {
                    let Some(id) = self.resolve(target, None) else { continue };
                    let gone = self.subtree(&id);
                    self.nodes.retain(|n| !gone.contains(&n.id));
                    self.edges.retain(|e| !gone.contains(&e.from) && !gone.contains(&e.to));
                }
            }
        }
        self.restore_pinned(pinned);
        let used: HashSet<String> = self.nodes.iter().filter_map(|n| n.group.clone()).collect();
        self.groups.retain(|x| used.contains(&x.id));
    }
}

fn utf16_prefix(text: &str, units: usize) -> String {
    let kept: Vec<u16> = text.encode_utf16().take(units).collect();
    String::from_utf16_lossy(&kept)
}

impl NodeData {
    /// Where an imported block sits on its page, as left, top, width and height.
    pub fn at(&self) -> Option<[f64; 4]> {
        let at = self.extra.get("at")?;
        Some([at["x"].as_f64()?, at["y"].as_f64()?, at["w"].as_f64()?, at["h"].as_f64()?])
    }

    /// The text layer of an imported block.
    pub fn rows(&self) -> Vec<RegionRow> {
        self.extra.get("rows").and_then(|r| serde_json::from_value(r.clone()).ok()).unwrap_or_default()
    }

    pub fn spans(&self) -> Vec<Span> {
        self.extra.get("spans").and_then(|s| serde_json::from_value(s.clone()).ok()).unwrap_or_default()
    }

    fn add_span(&mut self, span: Span) {
        let mut spans = self.spans();
        if !spans.iter().any(|s| s.same(&span)) {
            spans.push(span);
            self.extra.insert("spans".into(), json!(spans));
        }
    }
}

fn looks_like_id(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('n' | 'g')) && !s[1..].is_empty() && chars.all(|c| c.is_ascii_digit())
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ref {
    Id(String),
    Name(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    N(String),
    T(String),
    L(Ref, String),
    /// Question or formula, optionally beside something.
    Attached(Kind, Option<Ref>, String),
    K(Ref, String),
    /// A mark, optionally on a quoted word or, if the flag is set, the sentence around it.
    M(Ref, String, Option<(String, bool)>),
    H(Ref),
    I(Ref, String),
    E(Ref, Ref, Option<String>),
    X(Ref, Ref),
    R(Ref, String),
    A(Ref, String),
    G(String, Vec<Ref>),
    D(Ref),
}

struct Tok {
    quoted: bool,
    text: String,
}

fn tokens(line: &str) -> Vec<Tok> {
    let mut out = vec![];
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        if let Some(body) = rest.strip_prefix('「') {
            if let Some(end) = body.find('」') {
                out.push(Tok { quoted: true, text: body[..end].trim().to_string() });
                rest = body[end + '」'.len_utf8()..].trim_start();
                continue;
            }
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, tail) = rest.split_at(end);
        // Arrows and colons only exist to guide the model.
        if word != "->" && word != ":" {
            out.push(Tok { quoted: false, text: word.to_string() });
        }
        rest = tail.trim_start();
    }
    out
}

fn reference(t: Option<&Tok>) -> Option<Ref> {
    let t = t.filter(|t| !t.text.is_empty())?;
    Some(if t.quoted { Ref::Name(t.text.clone()) } else { Ref::Id(t.text.clone()) })
}

/// Parse model output. Bad lines are skipped so one error does not ruin the batch.
pub fn parse_ops(text: &str) -> Vec<Op> {
    let mut ops = vec![];
    for raw in text.split('\n') {
        let line = raw.trim();
        if line.is_empty() || line == "-" || line == "END" {
            continue;
        }
        let toks = tokens(line);
        let Some((head, rest)) = toks.split_first() else { continue };
        if head.quoted {
            continue;
        }
        let a = reference(rest.first());
        let b = reference(rest.get(1));
        let first = rest.first().map(|t| t.text.clone()).filter(|t| !t.is_empty());
        let second = rest.get(1).map(|t| t.text.clone()).filter(|t| !t.is_empty());
        let op = match head.text.as_str() {
            "N" => first.map(Op::N),
            "T" => first.map(Op::T),
            "H" => a.map(Op::H),
            "D" => a.map(Op::D),
            "E" => a.zip(b).map(|(a, b)| Op::E(a, b, rest.get(2).map(|t| t.text.clone()).filter(|t| !t.is_empty()))),
            "X" => a.zip(b).map(|(a, b)| Op::X(a, b)),
            "R" => a.zip(second).map(|(a, s)| Op::R(a, s)),
            "A" => a.zip(second).map(|(a, s)| Op::A(a, s)),
            "L" => a.zip(second).map(|(a, s)| Op::L(a, s)),
            "K" => a.zip(second).map(|(a, s)| Op::K(a, s)),
            "Q" | "F" => {
                let kind = if head.text == "Q" { Kind::Question } else { Kind::Formula };
                if rest.len() >= 2 && second.is_some() {
                    second.map(|s| Op::Attached(kind, a, s))
                } else {
                    first.map(|s| Op::Attached(kind, None, s))
                }
            }
            "M" => a.zip(second.filter(|s| MARKS.contains(&s.as_str()))).map(|(a, s)| {
                let sentence = rest.get(2).is_some_and(|t| t.text == "句" && !t.quoted);
                let quote = rest.get(if sentence { 3 } else { 2 }).filter(|t| t.quoted && !t.text.is_empty());
                Op::M(a, s, quote.map(|t| (t.text.clone(), sentence)))
            }),
            "I" => a.zip(second.filter(|s| ICONS.contains(&s.as_str()))).map(|(a, s)| Op::I(a, s)),
            "G" if rest.len() > 1 => {
                first.map(|g| Op::G(g, rest[1..].iter().filter_map(|t| reference(Some(t))).collect()))
            }
            _ => None,
        };
        ops.extend(op);
    }
    ops
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Utterance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
    /// What was circled on the board while this was said, as node ids, first circled first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pointing: Vec<String>,
}

impl Utterance {
    fn line(&self) -> String {
        self.speaker.as_ref().map_or_else(|| self.text.clone(), |s| format!("{s}：{}", self.text))
    }
}

pub fn is_han(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2E99 | 0x2E9B..=0x2EF3 | 0x2F00..=0x2FD5 | 0x3005 | 0x3007 | 0x3021..=0x3029
        | 0x3038..=0x303B | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFA6D | 0xFA70..=0xFAD9
        | 0x20000..=0x323AF)
}

/// Han characters weigh 6:1 against Latin letters, so a Chinese meeting full of English terms stays Chinese.
pub fn is_english<'a>(texts: impl Iterator<Item = &'a str>) -> bool {
    let (mut han, mut latin) = (0_usize, 0_usize);
    for t in texts {
        han += t.chars().filter(|c| is_han(*c)).count();
        latin += t.chars().filter(char::is_ascii_alphabetic).count();
    }
    latin > 0 && han * 6 < latin
}

pub fn board_is_english(recent: &[Utterance], next: &Utterance) -> bool {
    is_english(recent.iter().chain([next]).map(|u| u.text.as_str()))
}

const ENGLISH_BOARD: &str = "这场会说的是英文：板上的名字、条目、问题、备注都用英文写，照原话的叫法，不要翻译成中文。名字 1 到 4 个词，条目最多 8 个词。";

/// The most a line can point at; past that, the latest circled count.
pub const MAX_POINTED: usize = 8;

/// 「当前手指着：n4（库存服务）、n5（Redis）」, for what is still on the board, as the older app said it.
fn pointing_line(g: &Graph, ids: &[String]) -> Option<String> {
    let live: Vec<&Node> = ids.iter().filter_map(|id| g.nodes.iter().find(|n| &n.id == id)).collect();
    let shown: Vec<String> = live[live.len().saturating_sub(MAX_POINTED)..]
        .iter()
        .map(|n| {
            let name: String = n.label.chars().take(16).collect();
            let more = if n.label.chars().count() > 16 { "…" } else { "" };
            format!("{}（{name}{more}）", n.id)
        })
        .collect();
    (!shown.is_empty()).then(|| format!("当前手指着：{}", shown.join("、")))
}

pub fn user_message(g: &Graph, recent: &[Utterance], next: &Utterance) -> String {
    let mut parts = vec![];
    if board_is_english(recent, next) {
        parts.push(ENGLISH_BOARD.to_string());
    }
    parts.push(format!("白板：\n{}", g.describe()));
    if !recent.is_empty() {
        let lines: Vec<String> = recent.iter().map(|u| pointing_line(g, &u.pointing).map_or_else(|| u.line(), |p| format!("{p}\n{}", u.line()))).collect();
        parts.push(format!("前文（已经画过了）：\n{}", lines.join("\n")));
    }
    // Only when something is circled, so the prompt is otherwise exactly what the evaluation measured.
    if let Some(pointed) = pointing_line(g, &next.pointing) {
        parts.push(pointed);
    }
    parts.push(format!("新的一句：{}", next.line()));
    parts.join("\n")
}

/// GBNF built from the current board, so the model cannot name a node that does not exist.
pub fn grammar(g: &Graph, max_ops: usize, english: bool) -> String {
    let name_max = if english { 48 } else { 20 };
    let mut ids: Vec<String> = g.nodes.iter().map(|n| format!("\"{}\"", n.id)).collect();
    // Longest first, so n1 cannot take the prefix of n12.
    ids.sort_by_key(|id| std::cmp::Reverse(id.len()));
    let reference = if ids.is_empty() { "ref ::= name".to_string() } else { format!("ref ::= name | {}", ids.join(" | ")) };
    let alt = |xs: &[&str]| xs.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(" | ");
    [
        format!("root ::= \"-\" | op (\"\\n\" op){{0,{}}}", max_ops - 1),
        "op ::= e | x | gr | t | l | q | f | k | m | i | h | r | a | d".into(),
        "e ::= \"E \" ref \" -> \" ref (\" : \" name)?".into(),
        "x ::= \"X \" ref \" -> \" ref".into(),
        "gr ::= \"G \" name (\" \" ref){1,6}".into(),
        "t ::= \"T \" name".into(),
        "l ::= \"L \" ref \" \" name".into(),
        "q ::= \"Q \" (ref \" \")? name".into(),
        "f ::= \"F \" (ref \" \")? name".into(),
        "k ::= \"K \" ref \" \" name".into(),
        format!("m ::= \"M \" ref \" \" ({}) (\" \" (\"句 \")? quote)?", alt(&MARKS)),
        "quote ::= \"「\" [^「」\\n]{1,40} \"」\"".into(),
        format!("i ::= \"I \" ref \" \" ({})", alt(&ICONS)),
        "h ::= \"H \" ref".into(),
        "r ::= \"R \" ref \" \" name".into(),
        "a ::= \"A \" ref \" \" name".into(),
        "d ::= \"D \" ref".into(),
        reference,
        format!("name ::= \"「\" [^「」\\n]{{1,{name_max}}} \"」\""),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    mod pointing {
        use super::super::*;

        #[test]
        fn what_is_circled_is_named_only_while_it_is_on_the_board() {
            let mut g = Graph::default();
            g.apply(&parse_ops("N 库存服务\nN Redis"));
            let ids: Vec<String> = g.nodes.iter().map(|n| n.id.clone()).collect();
            let next = Utterance { speaker: None, text: "连起来".into(), pointing: vec![ids[0].clone(), ids[1].clone(), "n99".into()] };
            let message = user_message(&g, &[], &next);
            assert!(message.contains(&format!("当前手指着：{}（库存服务）、{}（Redis）\n新的一句：连起来", ids[0], ids[1])), "{message}");
            let plain = Utterance { speaker: None, text: "连起来".into(), pointing: vec![] };
            assert!(!user_message(&g, &[], &plain).contains("手指着"));
        }
    }

    mod imported_pages {
        use super::super::*;
        use crate::board_layout::StableLayout;
        use crate::steps::{diff, patch};
        use crate::sugiyama::first_diagram;
        use serde_json::Value;

        #[test]
        fn pages_match_the_typescript_engine() {
            let goldens: Value = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
            let cases = goldens["pages"].as_array().unwrap();
            assert!(cases.len() >= 12);
            for case in cases {
                let name = case["name"].as_str().unwrap();
                let before: Graph = serde_json::from_value(case["before"].clone()).unwrap();
                let mut g = before.clone();
                g.apply(&parse_ops(case["ops"].as_str().unwrap()));
                assert_eq!(serde_json::to_value(&g).unwrap(), case["graph"], "graph, {name}");
                assert_eq!(g.describe(), case["described"], "description, {name}");
                match diff(&before, &g) {
                    Some(d) => assert_eq!(patch(&before, &d).nodes, g.nodes, "step, {name}"),
                    None => assert!(case["step"].is_null(), "step, {name}"),
                }
                // The flow chart has its own layout here, unlike the older app's ELK.
                if name.contains("->") {
                    continue;
                }
                let mut memory = StableLayout::default();
                let l = memory.layout(&g, &mut first_diagram);
                let near = |a: f64, b: &Value, what: &str| assert!((a - b.as_f64().unwrap()).abs() < 0.01, "{what}: {a} vs {b}, {name}");
                let b = &case["bounds"];
                near(l.bounds.x0, &b["x0"], "x0");
                near(l.bounds.y0, &b["y0"], "y0");
                near(l.bounds.x1, &b["x1"], "x1");
                near(l.bounds.y1, &b["y1"], "y1");
                let anchors = case["anchors"].as_object().unwrap();
                assert_eq!(l.anchors.len(), anchors.len(), "anchors, {name}");
                for (id, a) in anchors {
                    let r = &l.anchors[id];
                    for (v, e) in [r.x, r.y, r.w, r.h].iter().zip(a.as_array().unwrap()) {
                        near(*v, e, id);
                    }
                }
                let regions = case["regions"].as_array().unwrap();
                assert_eq!(l.regions.len(), regions.len(), "regions, {name}");
                for (a, e) in l.regions.iter().zip(regions) {
                    assert_eq!(a.rect.id, e["id"], "{name}");
                    near(a.rect.x, &e["x"], "region x");
                    near(a.rect.y, &e["y"], "region y");
                    let rows = e["rows"].as_array().unwrap();
                    assert_eq!(a.rows.len(), rows.len(), "{name}");
                    for (row, er) in a.rows.iter().zip(rows) {
                        assert_eq!(row.id, er[0], "{name}");
                        assert_eq!(row.prefix, er[1], "{name}");
                        assert_eq!(row.text, er[2], "{name}");
                        near(row.x, &er[3], "row x");
                        near(row.y, &er[4], "row y");
                    }
                }
            }
        }
    }

    use super::*;

    #[derive(Deserialize)]
    struct Shape {
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        groups: Vec<Group>,
    }

    #[derive(Deserialize, Clone)]
    struct GRect {
        id: String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    #[derive(Deserialize)]
    struct GTitle {
        text: String,
        x: f64,
        w: f64,
    }

    #[derive(Deserialize)]
    struct GRow {
        id: String,
        prefix: String,
        text: String,
        x: f64,
        y: f64,
        w: f64,
        size: f64,
    }

    #[derive(Deserialize)]
    struct GRoute {
        from: String,
        to: String,
        points: Option<Vec<[f64; 2]>>,
    }

    #[derive(Deserialize)]
    struct GRegion {
        id: String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        title: Option<GTitle>,
        rows: Vec<GRow>,
    }

    #[derive(Deserialize)]
    struct GGroup {
        id: String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        label: String,
    }

    #[derive(Deserialize)]
    struct GDiagram {
        #[serde(rename = "box")]
        frame: GRect,
        nodes: Vec<GRect>,
        groups: Vec<GGroup>,
        edges: Vec<GRoute>,
    }

    #[derive(Deserialize)]
    struct GBounds {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    }

    #[derive(Deserialize)]
    struct GLayout {
        regions: Vec<GRegion>,
        diagram: Option<GDiagram>,
        links: Vec<GRoute>,
        bounds: GBounds,
    }

    #[derive(Deserialize)]
    struct Step {
        memory: Value,
        diff: Value,
        layout: GLayout,
        file: String,
        text: String,
        speaker: Option<String>,
        ops: String,
        describe: String,
        user: String,
        lang: String,
        grammar: String,
        after: Shape,
    }

    #[derive(Deserialize)]
    struct Stress {
        seed: u32,
        ops: String,
        layout: GLayout,
    }

    #[derive(Deserialize)]
    struct Goldens {
        steps: Vec<Step>,
        stress: Vec<Stress>,
    }

    /// Replays real model output and requires each step to match the TypeScript engine exactly.
    #[test]
    fn matches_the_typescript_engine_on_recorded_meetings() {
        let goldens: Goldens = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        assert!(goldens.steps.len() > 100);
        let mut g = Graph::default();
        let mut recent: Vec<Utterance> = vec![];
        let mut file = String::new();
        for (i, s) in goldens.steps.iter().enumerate() {
            if s.file != file {
                file.clone_from(&s.file);
                g = Graph::default();
                recent.clear();
            }
            let at = format!("step {i} of {}: {}", s.file, s.text);
            let next = Utterance { speaker: s.speaker.clone(), text: s.text.clone(), pointing: vec![] };
            let context = &recent[recent.len().saturating_sub(2)..];
            assert_eq!(g.describe(), s.describe, "describe, {at}");
            assert_eq!(user_message(&g, context, &next), s.user, "user message, {at}");
            let english = board_is_english(context, &next);
            assert_eq!(english, s.lang == "en", "language, {at}");
            assert_eq!(grammar(&g, 8, english), s.grammar, "grammar, {at}");
            g.apply(&parse_ops(&s.ops));
            assert_eq!(g.nodes, s.after.nodes, "nodes, {at}");
            assert_eq!(g.edges, s.after.edges, "edges, {at}");
            assert_eq!(g.groups, s.after.groups, "groups, {at}");
            recent.push(next);
        }
    }

    fn close(a: f64, b: f64, what: &str) {
        assert!((a - b).abs() < 0.01, "{what}: {a} vs {b}");
    }

    fn elk_diagram(d: &GDiagram) -> crate::board_layout::Diagram {
        use crate::board_layout::{Diagram, Rect, Route};
        let (ox, oy) = (d.frame.x, d.frame.y);
        let rect = |id: &str, x: f64, y: f64, w: f64, h: f64| Rect { id: id.to_string(), x: x - ox, y: y - oy, w, h };
        Diagram {
            x0: 0.0,
            y0: 0.0,
            w: d.frame.w,
            h: d.frame.h,
            nodes: d.nodes.iter().map(|b| rect(&b.id, b.x, b.y, b.w, b.h)).collect(),
            groups: d.groups.iter().map(|b| (rect(&b.id, b.x, b.y, b.w, b.h), b.label.clone())).collect(),
            edges: d
                .edges
                .iter()
                .map(|e| Route {
                    from: e.from.clone(),
                    to: e.to.clone(),
                    points: e.points.as_ref().map_or(vec![], |p| p.iter().map(|q| [q[0] - ox, q[1] - oy]).collect()),
                    label_box: None,
                })
                .collect(),
        }
    }

    /// With ELK's own first placement injected, everything else (topic regions, free spots,
    /// incremental nodes, routes, links, bounds) must equal the TypeScript layout at every step.
    #[test]
    fn layout_matches_typescript_given_the_same_first_diagram() {
        use crate::board_layout::StableLayout;
        let goldens: Goldens = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        let mut g = Graph::default();
        let mut board = StableLayout::default();
        let mut file = String::new();
        let mut compared = 0;
        let mut previous_board = Graph::default();
        // Numbers differ only in float noise, so rows compare at the precision the layout cares about.
        fn rounded(v: &Value) -> String {
            match v {
                Value::Number(n) => format!("{:.2}", n.as_f64().unwrap()),
                Value::Array(a) => format!("[{}]", a.iter().map(rounded).collect::<Vec<_>>().join(",")),
                Value::Object(o) => {
                    let mut rows: Vec<String> = o.iter().map(|(k, v)| format!("{k}:{}", rounded(v))).collect();
                    rows.sort();
                    format!("{{{}}}", rows.join(","))
                }
                other => other.to_string(),
            }
        }
        for (i, s) in goldens.steps.iter().enumerate() {
            if s.file != file {
                file.clone_from(&s.file);
                g = Graph::default();
                board = StableLayout::default();
                previous_board = Graph::default();
            }
            let at = format!("step {i} of {}: {}", s.file, s.text);
            g.apply(&parse_ops(&s.ops));
            // The positions the TypeScript layout remembered must work here too.
            if let Some(previous) = i.checked_sub(1).map(|p| &goldens.steps[p]).filter(|p| p.file == s.file) {
                board = StableLayout::default();
                board.restore(&previous.memory);
            }
            let l = board.layout(&g, &mut |_, _| elk_diagram(s.layout.diagram.as_ref().expect("first diagram")));
            let (mine, theirs) = (board.snapshot(), &s.memory);
            for key in ["regionAt", "nodeAt", "routeAt", "boxAt"] {
                let sorted = |v: &Value| {
                    let mut rows: Vec<String> = v[key].as_array().unwrap().iter().map(rounded).collect();
                    rows.sort();
                    rows
                };
                assert_eq!(sorted(&mine), sorted(theirs), "memory {key}, {at}");
            }
            assert!(g.nodes.iter().all(|n| n.ink == "ink"), "{at}");
            assert_eq!(serde_json::to_value(crate::steps::diff(&previous_board, &g)).unwrap(), s.diff, "step, {at}");
            previous_board = g.clone();
            assert_eq!(l.regions.len(), s.layout.regions.len(), "regions, {at}");
            for (a, b) in l.regions.iter().zip(&s.layout.regions) {
                assert_eq!(a.rect.id, b.id, "region id, {at}");
                for (x, y, what) in [(a.rect.x, b.x, "x"), (a.rect.y, b.y, "y"), (a.rect.w, b.w, "w"), (a.rect.h, b.h, "h")] {
                    close(x, y, &format!("region {} {what}, {at}", b.id));
                }
                assert_eq!(a.title.as_ref().map(|t| t.text.as_str()), b.title.as_ref().map(|t| t.text.as_str()), "title, {at}");
                if let (Some(p), Some(q)) = (&a.title, &b.title) {
                    close(p.x, q.x, "title x");
                    close(p.w, q.w, "title w");
                }
                assert_eq!(a.rows.len(), b.rows.len(), "rows, {at}");
                for (p, q) in a.rows.iter().zip(&b.rows) {
                    assert_eq!((&p.id, &p.prefix, &p.text), (&q.id, &q.prefix, &q.text), "row, {at}");
                    for (x, y) in [(p.x, q.x), (p.y, q.y), (p.w, q.w), (p.size, q.size)] {
                        close(x, y, &format!("row {}, {at}", q.id));
                    }
                }
            }
            assert_eq!(l.diagram.is_some(), s.layout.diagram.is_some(), "diagram, {at}");
            if let (Some((frame, d)), Some(e)) = (&l.diagram, &s.layout.diagram) {
                for (x, y) in [(frame.x, e.frame.x), (frame.y, e.frame.y), (frame.w, e.frame.w), (frame.h, e.frame.h)] {
                    close(x, y, &format!("frame, {at}"));
                }
                assert_eq!(d.nodes.len(), e.nodes.len(), "nodes, {at}");
                for b in &e.nodes {
                    let a = d.nodes.iter().find(|n| n.id == b.id).unwrap_or_else(|| panic!("node {}, {at}", b.id));
                    for (x, y) in [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)] {
                        close(x, y, &format!("node {}, {at}", b.id));
                    }
                }
                assert_eq!(d.groups.len(), e.groups.len(), "groups, {at}");
                for (b, (a, label)) in e.groups.iter().zip(&d.groups) {
                    assert_eq!((&a.id, label), (&b.id, &b.label), "group, {at}");
                    for (x, y) in [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)] {
                        close(x, y, &format!("group {}, {at}", b.id));
                    }
                }
                assert_eq!(d.edges.len(), e.edges.len(), "edges, {at}");
                for (a, b) in d.edges.iter().zip(&e.edges) {
                    let pts = b.points.as_ref().unwrap();
                    assert_eq!(a.points.len(), pts.len(), "route {}>{}, {at}", b.from, b.to);
                    for (p, q) in a.points.iter().zip(pts) {
                        close(p[0], q[0], &format!("route, {at}"));
                        close(p[1], q[1], &format!("route, {at}"));
                    }
                }
            }
            assert_eq!(l.links.len(), s.layout.links.len(), "links, {at}");
            for ((from, to, route), b) in l.links.iter().zip(&s.layout.links) {
                assert_eq!((from, to), (&b.from, &b.to), "link, {at}");
                assert_eq!(route.is_some(), b.points.is_some(), "link route, {at}");
            }
            for (x, y) in [(l.bounds.x0, s.layout.bounds.x0), (l.bounds.y0, s.layout.bounds.y0), (l.bounds.x1, s.layout.bounds.x1), (l.bounds.y1, s.layout.bounds.y1)] {
                close(x, y, &format!("bounds, {at}"));
            }
            compared += 1;
        }
        assert!(compared > 100);
    }

    /// The objective defects of a finished board, comparable between two layouts.
    #[derive(Default, Debug, Clone, Copy)]
    struct Defects {
        node_overlaps: usize,
        edges_through_nodes: usize,
        crossings: usize,
        bends: usize,
        length: f64,
    }

    fn measure(nodes: &[crate::board_layout::Rect], edges: &[(String, String, Vec<[f64; 2]>)]) -> Defects {
        use crate::board_layout::overlaps;
        let mut d = Defects::default();
        for (i, a) in nodes.iter().enumerate() {
            d.node_overlaps += nodes[i + 1..].iter().filter(|b| overlaps(a, b, 0.0)).count();
        }
        for (from, to, pts) in edges {
            d.bends += pts.len().saturating_sub(2);
            d.length += pts.windows(2).map(|s| (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1])).sum::<f64>();
            for b in nodes.iter().filter(|b| &b.id != from && &b.id != to) {
                let hit = pts.windows(2).any(|s| {
                    (0..=40).any(|k| {
                        let t = f64::from(k) / 40.0;
                        let (x, y) = (s[0][0] + (s[1][0] - s[0][0]) * t, s[0][1] + (s[1][1] - s[0][1]) * t);
                        x > b.x + 4.0 && x < b.x + b.w - 4.0 && y > b.y + 4.0 && y < b.y + b.h - 4.0
                    })
                });
                d.edges_through_nodes += usize::from(hit);
            }
        }
        let segs: Vec<(usize, [f64; 2], [f64; 2])> = edges.iter().enumerate().flat_map(|(i, e)| e.2.windows(2).map(move |s| (i, s[0], s[1]))).collect();
        for (i, a) in segs.iter().enumerate() {
            for b in &segs[i + 1..] {
                if a.0 == b.0 || edges[a.0].0 == edges[b.0].0 || edges[a.0].1 == edges[b.0].1 || edges[a.0].1 == edges[b.0].0 || edges[a.0].0 == edges[b.0].1 {
                    continue;
                }
                let cross = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0]);
                if cross(a.1, a.2, b.1) * cross(a.1, a.2, b.2) < 0.0 && cross(b.1, b.2, a.1) * cross(b.1, b.2, a.2) < 0.0 {
                    d.crossings += 1;
                }
            }
        }
        d
    }

    /// The stand-in for ELK must never overlap nodes or run an edge through one, on any real
    /// recorded board. Crossings and length are reported against ELK's for comparison.
    #[test]
    fn first_diagram_is_comparable_to_elk() {
        use crate::board_layout::{Rect, StableLayout};
        let goldens: Goldens = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        let mut by_file: Vec<(&str, Vec<&Step>)> = vec![];
        for s in &goldens.steps {
            match by_file.last_mut() {
                Some((f, v)) if *f == s.file => v.push(s),
                _ => by_file.push((&s.file, vec![s])),
            }
        }
        let (mut ours_all, mut elk_all) = (Defects::default(), Defects::default());
        let mut boards = 0;
        for (file, steps) in by_file {
            let mut g = Graph::default();
            let mut board = StableLayout::default();
            let mut layout = None;
            let mut elk = None;
            for s in steps {
                g.apply(&parse_ops(&s.ops));
                layout = Some(board.layout(&g, &mut crate::sugiyama::first_diagram));
                elk = s.layout.diagram.as_ref();
            }
            let (Some(layout), Some(elk)) = (layout, elk) else { continue };
            let (_, d) = layout.diagram.as_ref().expect("both have a flow");
            let ours = measure(&d.nodes, &d.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.points.clone())).collect::<Vec<_>>());
            let theirs_nodes: Vec<Rect> = elk.nodes.iter().map(|b| Rect { id: b.id.clone(), x: b.x, y: b.y, w: b.w, h: b.h }).collect();
            let theirs = measure(&theirs_nodes, &elk.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.points.clone().unwrap())).collect::<Vec<_>>());
            println!("{file}: ours {ours:?} elk {theirs:?} size ours {:.0}x{:.0} elk {:.0}x{:.0}", d.w, d.h, elk.frame.w, elk.frame.h);
            for (t, v) in [(&mut ours_all, ours), (&mut elk_all, theirs)] {
                t.node_overlaps += v.node_overlaps;
                t.edges_through_nodes += v.edges_through_nodes;
                t.crossings += v.crossings;
                t.bends += v.bends;
                t.length += v.length;
            }
            boards += 1;
        }
        println!("TOTAL over {boards} boards: ours {ours_all:?} elk {elk_all:?}");
        assert!(boards >= 10);
        assert_eq!(ours_all.node_overlaps, 0, "nodes overlap");
        assert!(ours_all.edges_through_nodes <= elk_all.edges_through_nodes, "edges through nodes: ours {} elk {}", ours_all.edges_through_nodes, elk_all.edges_through_nodes);
    }

    /// Dense random graphs with cycles and groups, each placed whole by ELK and by the stand-in.
    #[test]
    fn dense_graphs_hold_up_against_elk() {
        use crate::board_layout::{overlaps, Rect, StableLayout};
        let goldens: Goldens = serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        let (mut ours_all, mut elk_all) = (Defects::default(), Defects::default());
        let (mut ours_area, mut elk_area) = (0.0, 0.0);
        let (mut ours_frames, mut elk_frames) = (0_usize, 0_usize);
        let (mut worse, mut better) = (0, 0);
        let frames_hit = |groups: &[Rect], nodes: &[(Rect, bool)]| nodes.iter().filter(|(n, member)| !member && groups.iter().any(|f| overlaps(n, f, 0.0))).count();
        for s in &goldens.stress {
            let mut g = Graph::default();
            g.apply(&parse_ops(&s.ops));
            let l = StableLayout::default().layout(&g, &mut crate::sugiyama::first_diagram);
            let (_, d) = l.diagram.as_ref().expect("flow");
            let elk = s.layout.diagram.as_ref().expect("flow");
            let ours = measure(&d.nodes, &d.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.points.clone())).collect::<Vec<_>>());
            let rects: Vec<Rect> = elk.nodes.iter().map(|b| Rect { id: b.id.clone(), x: b.x, y: b.y, w: b.w, h: b.h }).collect();
            let theirs = measure(&rects, &elk.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.points.clone().unwrap())).collect::<Vec<_>>());
            let member = |id: &str| g.nodes.iter().any(|n| n.id == id && n.group.is_some());
            ours_frames += frames_hit(&d.groups.iter().map(|(b, _)| b.clone()).collect::<Vec<_>>(), &d.nodes.iter().map(|n| (n.clone(), member(&n.id))).collect::<Vec<_>>());
            elk_frames += frames_hit(&elk.groups.iter().map(|b| Rect { id: b.id.clone(), x: b.x, y: b.y, w: b.w, h: b.h }).collect::<Vec<_>>(), &rects.iter().map(|n| (n.clone(), member(&n.id))).collect::<Vec<_>>());
            let score = |x: &Defects| x.node_overlaps * 100 + x.edges_through_nodes * 10 + x.crossings;
            match score(&ours).cmp(&score(&theirs)) {
                std::cmp::Ordering::Greater => worse += 1,
                std::cmp::Ordering::Less => better += 1,
                std::cmp::Ordering::Equal => {}
            }
            println!("seed {} nodes {}: ours {ours:?} {:.0}x{:.0} | elk {theirs:?} {:.0}x{:.0}", s.seed, d.nodes.len(), d.w, d.h, elk.frame.w, elk.frame.h);
            ours_area += d.w * d.h;
            elk_area += elk.frame.w * elk.frame.h;
            for (t, v) in [(&mut ours_all, ours), (&mut elk_all, theirs)] {
                t.node_overlaps += v.node_overlaps;
                t.edges_through_nodes += v.edges_through_nodes;
                t.crossings += v.crossings;
                t.bends += v.bends;
                t.length += v.length;
            }
        }
        println!("STRESS over {} graphs: ours {ours_all:?} elk {elk_all:?}", goldens.stress.len());
        println!("STRESS area ratio {:.2}, strangers inside frames ours {ours_frames} elk {elk_frames}, worse on {worse} better on {better}", ours_area / elk_area);
        assert_eq!(ours_all.node_overlaps, 0);
        assert_eq!(ours_all.edges_through_nodes, 0, "edges through nodes");
        assert_eq!(ours_frames, 0, "strangers inside group frames");
    }

    #[test]
    fn nodes_keep_their_field_order_and_new_fields_go_last() {
        let text = r#"{"label":"甲","id":"n1","marks":[],"by":"A","kind":"node","aliases":[],"ink":"ink"}"#;
        let mut node: Node = serde_json::from_str(text).unwrap();
        assert_eq!(serde_json::to_string(&node).unwrap(), text);
        node.parent = Some("n9".into());
        node.marks.push("星标".into());
        assert_eq!(serde_json::to_string(&node).unwrap(), r#"{"label":"甲","id":"n1","marks":["星标"],"by":"A","kind":"node","aliases":[],"ink":"ink","parent":"n9"}"#);
    }

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let raw = serde_json::json!({
            "nodes": [{ "id": "n1", "label": "甲", "aliases": [], "kind": "region", "marks": [], "ink": "draft", "by": "A", "at": { "x": 1 }, "rows": [] }],
            "edges": [{ "from": "n1", "to": "n1", "ink": "ink", "by": "B" }],
            "groups": [],
            "nextNode": 2,
            "nextGroup": 1,
            "underlay": { "w": 10, "h": 20 },
        });
        let g: Graph = serde_json::from_value(raw.clone()).unwrap();
        assert!(g.underlay().is_some());
        assert_eq!(serde_json::to_value(&g).unwrap(), raw);
    }

    #[test]
    fn toggling_a_mark_adds_then_removes_it_and_ignores_unknowns() {
        let mut g = Graph::default();
        g.apply(&parse_ops("T 「待办」"));
        let id = g.nodes[0].id.clone();
        g.toggle_mark(&id, "星标");
        assert_eq!(g.nodes[0].marks, ["星标"]);
        g.toggle_mark(&id, "划掉");
        g.toggle_mark(&id, "星标");
        assert_eq!(g.nodes[0].marks, ["划掉"]);
        g.toggle_mark(&id, "不存在的标记");
        g.toggle_mark("n99", "星标");
        assert_eq!(g.nodes[0].marks, ["划掉"]);
    }

    #[test]
    fn bad_lines_and_unknown_ids_change_nothing() {
        let mut g = Graph::default();
        g.apply(&parse_ops("E n99 -> n98\nM n7 划掉\nZ 「怪」\n「只有名字」\nM 「无」 不存在的标记"));
        assert!(g.nodes.is_empty() && g.edges.is_empty());
    }
}
