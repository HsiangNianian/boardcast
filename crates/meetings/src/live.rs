//! The meeting a window has open: the board, what was said, undo, and the replay record.
//! It keeps the file's raw JSON and overwrites only the fields it owns.

use crate::store::{new_id, Loaded, Origin};
use boardcast_board::graph::{Graph, Kind};
use boardcast_board::steps::{diff, ink_only, replay};
use serde_json::{json, Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const UNTITLED: &str = "Untitled meeting";
const UNDO_LIMIT: usize = 100;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

/// The undo stacks, as in src/engine/edit.ts.
pub struct History<T> {
    past: Vec<T>,
    future: Vec<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self { past: vec![], future: vec![] }
    }
}

impl<T> History<T> {
    /// Remember the state before a change. A new change ends what could be redone.
    pub fn record(&mut self, before: T) {
        self.past.push(before);
        if self.past.len() > UNDO_LIMIT {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn undo(&mut self, current: T) -> Option<T> {
        let previous = self.past.pop()?;
        self.future.push(current);
        Some(previous)
    }

    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.future.pop()?;
        self.past.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

/// Why a meeting opens without editing. Its file is left exactly as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewOnly {
    /// The board could not be read.
    Unreadable,
    /// Saved by a newer version.
    NewerFormat,
    /// Imported pages or free-edit content, which this version cannot edit yet.
    ImportedContent,
}

/// One undo position: the board, and where everything was placed.
#[derive(Clone)]
pub struct Snap {
    pub graph: Graph,
    pub memory: Value,
}

pub struct Live {
    raw: Map<String, Value>,
    pub origin: Origin,
    pub revision: u64,
    pub graph: Graph,
    pub memory: Value,
    pub lines: Vec<Value>,
    pub history: Vec<Value>,
    pub steps: Vec<Value>,
    /// What the replay record has reached, to diff the next change against.
    recorded: Graph,
    undo: History<Snap>,
    /// Bumped by every change, so a save knows whether it caught up.
    pub changes: u64,
    /// Why this meeting can only be viewed, if so.
    pub read_only: Option<ViewOnly>,
}

fn array(raw: &Map<String, Value>, key: &str) -> Vec<Value> {
    raw.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn scene_has_content(scene: Option<&Value>) -> bool {
    scene.is_some_and(|s| !s.is_null() && s["elements"].as_array().is_some_and(|e| !e.is_empty()))
}

impl Live {
    pub fn new() -> Self {
        let now = now_ms();
        let Value::Object(raw) = json!({ "version": 2, "id": new_id(), "title": UNTITLED, "createdAt": now, "updatedAt": now }) else {
            unreachable!("literal object")
        };
        Self::build(raw, Origin::New, 0)
    }

    pub fn from_loaded(loaded: Loaded) -> Self {
        Self::build(loaded.raw, loaded.origin, loaded.revision)
    }

    fn build(raw: Map<String, Value>, origin: Origin, revision: u64) -> Self {
        let graph: Result<Graph, _> = raw.get("graph").map_or_else(|| Ok(Graph::default()), |g| serde_json::from_value(g.clone()));
        let unreadable = graph.is_err();
        let graph = graph.unwrap_or_default();
        let mut lines = array(&raw, "lines");
        // A line still waiting for the model when the meeting closed will never be drawn.
        for line in &mut lines {
            if line["ms"].is_null() && !line["manual"].as_bool().unwrap_or(false) {
                line["ms"] = json!(0);
                line["failed"] = json!(true);
                line["boardFailure"] = json!("cancelled");
                line["ops"] = json!("");
            }
        }
        let mut steps = array(&raw, "steps");
        // Older meetings have no record of how the board grew, so the whole board counts as one step.
        if steps.is_empty() {
            if let Some(d) = diff(&Graph::default(), &ink_only(&graph)) {
                steps.push(json!({ "t": raw.get("updatedAt").cloned().unwrap_or(json!(0)), "line": lines.len(), "d": d }));
            }
        }
        let recorded = replay(&steps);
        let pages = raw.get("pages").and_then(Value::as_array);
        let imported = graph.has_imported_content()
            || pages.is_some_and(|p| {
                p.iter().any(|page| {
                    page.get("underlay").is_some_and(|u| !u.is_null())
                        || scene_has_content(page.get("scene"))
                        || page["graph"]["nodes"].as_array().is_some_and(|n| n.iter().any(|x| x["kind"] == "region"))
                })
            })
            || scene_has_content(raw.get("scene"));
        let read_only = if unreadable {
            Some(ViewOnly::Unreadable)
        } else if raw.get("version").and_then(Value::as_u64).unwrap_or(1) > crate::store::MEETING_VERSION {
            Some(ViewOnly::NewerFormat)
        } else if imported {
            Some(ViewOnly::ImportedContent)
        } else {
            None
        };
        Self {
            memory: raw.get("layout").cloned().unwrap_or(Value::Null),
            history: array(&raw, "history"),
            lines,
            steps,
            recorded,
            graph,
            undo: History::default(),
            changes: 0,
            read_only,
            raw,
            origin,
            revision,
        }
    }

    pub fn id(&self) -> &str {
        self.raw["id"].as_str().unwrap_or_default()
    }

    pub fn title(&self) -> &str {
        self.raw.get("title").and_then(Value::as_str).unwrap_or(UNTITLED)
    }

    pub fn has_content(&self) -> bool {
        !self.lines.is_empty() || !self.graph.nodes.is_empty() || !self.summary().is_empty()
    }

    /// This meeting must not leave the machine, so it only uses the local model.
    pub fn local_only(&self) -> bool {
        self.raw.get("localOnly").and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn pages(&self) -> usize {
        self.raw.get("pages").and_then(Value::as_array).map_or(1, |p| p.len().max(1))
    }

    pub fn page(&self) -> usize {
        self.raw.get("page").and_then(Value::as_u64).unwrap_or(0) as usize
    }

    /// Move to another board of this meeting, keeping the one being left.
    pub fn go_to_page(&mut self, page: usize) {
        if page >= self.pages() || page == self.page() {
            return;
        }
        self.store_page();
        let next = self.raw["pages"][page].clone();
        self.graph = serde_json::from_value(next["graph"].clone()).unwrap_or_default();
        self.memory = next.get("layout").cloned().unwrap_or(Value::Null);
        self.steps = next["steps"].as_array().cloned().unwrap_or_default();
        self.recorded = replay(&self.steps);
        self.undo = History::default();
        self.raw.insert("page".into(), json!(page));
    }

    /// The board on screen belongs in `pages` as well as the top level, as the older app keeps it.
    fn store_page(&mut self) {
        let page = self.page();
        let (graph, memory, steps) = (json!(self.graph), self.memory.clone(), json!(self.steps));
        if let Some(entry) = self.raw.get_mut("pages").and_then(|p| p.get_mut(page)).and_then(Value::as_object_mut) {
            entry.insert("graph".into(), graph);
            entry.insert("layout".into(), memory);
            entry.insert("steps".into(), steps);
        }
    }

    /// A line of speech or typing. Without `ms` it counts as still waiting for the model.
    pub fn add_line(&mut self, speaker: Option<&str>, text: &str, asr_ms: Option<u128>) {
        let mut line = json!({ "text": text, "page": self.page() });
        if let Some(speaker) = speaker {
            line["speaker"] = json!(speaker);
        }
        if let Some(ms) = asr_ms {
            line["heard"] = json!({ "asrMs": ms });
        }
        self.lines.push(line);
        self.touch();
    }

    /// A note about a hand-made change. Marked manual, so it is never read as something said.
    pub fn add_note(&mut self, text: &str) {
        self.lines.push(json!({ "text": text, "manual": true, "ms": 0, "page": self.page() }));
    }

    fn settle(&mut self, text: &str, update: impl Fn(&mut Value)) {
        // The earliest waiting line with this text: lines finish in the order they were said.
        if let Some(line) = self.lines.iter_mut().find(|l| l["ms"].is_null() && l["text"] == text) {
            update(line);
        }
    }

    /// The model finished this batch of lines and the board became `graph`.
    pub fn model_drew(&mut self, spoken: &[(Option<String>, String)], graph: Graph, memory: Value, ops: &str, llm_ms: u128) {
        for (i, (speaker, text)) in spoken.iter().enumerate() {
            // A merged batch produced one set of operations, shown on its last line.
            let shown = if i + 1 == spoken.len() { ops } else { "" };
            self.settle(text, |line| {
                line["ms"] = json!(llm_ms);
                line["ops"] = json!(shown);
            });
            self.remember(speaker.as_deref(), text);
        }
        self.change_board(graph, memory);
    }

    /// The model could not answer. The lines stay in the transcript, marked for a retry later.
    pub fn model_failed(&mut self, spoken: &[(Option<String>, String)]) {
        for (speaker, text) in spoken {
            self.settle(text, |line| {
                line["ms"] = json!(0);
                line["failed"] = json!(true);
                line["boardFailure"] = json!("request");
                line["ops"] = json!("");
            });
            self.remember(speaker.as_deref(), text);
        }
        self.touch();
    }

    fn remember(&mut self, speaker: Option<&str>, text: &str) {
        let mut said = json!({ "text": text });
        if let Some(speaker) = speaker {
            said["speaker"] = json!(speaker);
        }
        self.history.push(said);
    }

    /// Any change to the board: it can be undone, and replay records it.
    pub fn change_board(&mut self, graph: Graph, memory: Value) {
        if graph == self.graph {
            self.memory = memory;
            self.touch();
            return;
        }
        let before = Snap { graph: std::mem::replace(&mut self.graph, graph), memory: std::mem::replace(&mut self.memory, memory) };
        self.undo.record(before);
        self.record_step();
        self.refresh_title();
        self.touch();
    }

    fn record_step(&mut self) {
        let shown = ink_only(&self.graph);
        if let Some(d) = diff(&self.recorded, &shown) {
            self.steps.push(json!({ "t": now_ms(), "line": self.lines.len(), "d": d }));
            self.recorded = shown;
        }
    }

    pub fn can_undo(&self) -> bool {
        self.read_only.is_none() && self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.read_only.is_none() && self.undo.can_redo()
    }

    fn current(&self) -> Snap {
        Snap { graph: self.graph.clone(), memory: self.memory.clone() }
    }

    fn step_to(&mut self, snap: Snap) -> Snap {
        self.graph = snap.graph.clone();
        self.memory = snap.memory.clone();
        self.record_step();
        self.touch();
        snap
    }

    pub fn undo(&mut self) -> Option<Snap> {
        let current = self.current();
        let snap = self.undo.undo(current)?;
        Some(self.step_to(snap))
    }

    pub fn redo(&mut self) -> Option<Snap> {
        let current = self.current();
        let snap = self.undo.redo(current)?;
        Some(self.step_to(snap))
    }

    /// Every board of the meeting, with the one on screen as it is now.
    pub fn boards(&self) -> Vec<Graph> {
        self.board_pages().into_iter().map(|(graph, _)| graph).collect()
    }

    /// Every board with where its things were placed.
    pub fn board_pages(&self) -> Vec<(Graph, Value)> {
        let Some(pages) = self.raw.get("pages").and_then(Value::as_array).filter(|p| !p.is_empty()) else {
            return vec![(self.graph.clone(), self.memory.clone())];
        };
        let at = self.page();
        pages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == at {
                    (self.graph.clone(), self.memory.clone())
                } else {
                    (serde_json::from_value(p["graph"].clone()).unwrap_or_default(), p.get("layout").cloned().unwrap_or(Value::Null))
                }
            })
            .collect()
    }

    /// When the meeting began, in milliseconds.
    pub fn created_at(&self) -> Option<f64> {
        self.raw.get("createdAt").and_then(Value::as_f64)
    }

    pub fn summary(&self) -> &str {
        self.raw.get("summary").and_then(Value::as_str).unwrap_or_default()
    }

    /// The fingerprint of what the summary was made from.
    pub fn summary_source(&self) -> Option<&str> {
        self.raw.get("summarySource").and_then(Value::as_str)
    }

    /// An empty summary is no summary: the file then has neither field, as the older app writes it.
    pub fn set_summary(&mut self, text: &str, source: Option<&str>) {
        if text.is_empty() {
            self.raw.remove("summary");
            self.raw.remove("summarySource");
        } else {
            self.raw.insert("summary".into(), json!(text));
            match source {
                Some(source) => self.raw.insert("summarySource".into(), json!(source)),
                None => self.raw.remove("summarySource"),
            };
        }
        self.touch();
    }

    /// Older meetings have a summary but no fingerprint, so opening one takes the meeting as it stands.
    pub fn adopt_summary_source(&mut self, source: &str) {
        if !self.summary().is_empty() && self.summary_source().is_none() {
            self.raw.insert("summarySource".into(), json!(source));
        }
    }

    /// An unnamed meeting takes its title from the board: first topic, then first node, then first line.
    fn refresh_title(&mut self) {
        if self.raw.get("titled").and_then(Value::as_bool).unwrap_or(false) {
            return;
        }
        let name = self
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == Kind::Topic)
            .or_else(|| self.graph.nodes.first())
            .map(|n| n.label.clone())
            .or_else(|| self.lines.first().and_then(|l| l["text"].as_str()).map(|t| t.chars().take(20).collect()));
        if let Some(name) = name {
            self.raw.insert("title".into(), json!(name));
        }
    }

    fn touch(&mut self) {
        self.changes += 1;
    }

    /// The meeting as it should be written now.
    pub fn snapshot(&mut self) -> Map<String, Value> {
        self.store_page();
        let mut out = self.raw.clone();
        out.insert("updatedAt".into(), json!(now_ms()));
        out.insert("graph".into(), json!(self.graph));
        out.insert("layout".into(), self.memory.clone());
        out.insert("lines".into(), json!(self.lines));
        out.insert("history".into(), json!(self.history));
        out.insert("steps".into(), json!(self.steps));
        out
    }

    /// The saved copy has this revision; a later save builds on it.
    pub fn saved(&mut self, revision: u64) {
        self.revision = revision;
        self.origin = Origin::Current;
    }
}

impl Default for Live {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use boardcast_board::graph::parse_ops;
    use boardcast_board::steps::replay;

    fn with(graph: &Graph, ops: &str) -> Graph {
        let mut g = graph.clone();
        g.apply(&parse_ops(ops));
        g
    }

    fn loaded(value: Value, origin: Origin) -> Live {
        let Value::Object(raw) = value else { panic!("object") };
        Live::from_loaded(Loaded { raw, origin, revision: 0 })
    }

    #[test]
    fn a_summary_is_written_and_removed_as_the_older_app_does() {
        let mut live = Live::new();
        assert!(!live.has_content());
        let before = live.changes;
        live.set_summary("### 结论\n- 好", Some("abc-1"));
        assert!(live.has_content());
        assert!(live.changes > before, "it needs saving");
        let raw = live.snapshot();
        assert_eq!(raw["summary"], "### 结论\n- 好");
        assert_eq!(raw["summarySource"], "abc-1");
        live.set_summary("", Some("abc-1"));
        let raw = live.snapshot();
        assert!(!raw.contains_key("summary") && !raw.contains_key("summarySource"));
        assert_eq!(live.summary_source(), None);
    }

    #[test]
    fn a_summary_without_a_fingerprint_takes_one_only_on_opening() {
        let mut live = loaded(json!({ "id": "m", "summary": "旧摘要" }), Origin::Current);
        let changes = live.changes;
        live.adopt_summary_source("now-1");
        assert_eq!(live.summary_source(), Some("now-1"));
        assert_eq!(live.changes, changes, "opening a meeting is not a change");
        live.adopt_summary_source("later-2");
        assert_eq!(live.summary_source(), Some("now-1"));
        let mut empty = Live::new();
        empty.adopt_summary_source("now-1");
        assert_eq!(empty.summary_source(), None);
    }

    #[test]
    fn boards_cover_every_page_with_the_current_one_live() {
        let one = with(&Graph::default(), "T 「一」");
        let two = with(&Graph::default(), "T 「二」");
        let mut live = loaded(json!({ "id": "m", "graph": one, "page": 0, "pages": [{ "graph": one }, { "graph": two }] }), Origin::Current);
        assert_eq!(live.boards().len(), 2);
        let changed = with(&one, "T 「三」");
        live.change_board(changed.clone(), Value::Null);
        assert_eq!(live.boards()[0].nodes, changed.nodes);
        assert_eq!(live.boards()[1].nodes, two.nodes);
        assert_eq!(Live::new().boards().len(), 1);
    }

    #[test]
    fn a_meeting_grows_and_its_steps_replay_to_the_board() {
        let mut live = Live::new();
        live.add_line(Some("A"), "用户调网关", None);
        assert!(live.lines[0]["ms"].is_null());
        let g = with(&live.graph, "E 「用户」 -> 「网关」");
        live.model_drew(&[(Some("A".into()), "用户调网关".into())], g, json!({ "regionAt": [] }), "E 「用户」 -> 「网关」", 12);
        assert_eq!(live.lines[0]["ms"], 12);
        assert_eq!(live.history[0]["speaker"], "A");
        assert_eq!(live.title(), "用户");
        let raw = live.snapshot();
        assert_eq!(raw["graph"]["nodes"][0]["ink"], "ink");
        let replayed = replay(raw["steps"].as_array().unwrap());
        assert_eq!(replayed.nodes, live.graph.nodes);
        assert_eq!(replayed.edges, live.graph.edges);
    }

    #[test]
    fn undo_and_redo_move_the_board_and_stay_replayable() {
        let mut live = Live::new();
        let first = with(&live.graph, "T 「待办」");
        live.change_board(first.clone(), json!("m1"));
        let mut marked = first.clone();
        let id = marked.nodes[0].id.clone();
        marked.toggle_mark(&id, "星标");
        live.change_board(marked.clone(), json!("m2"));
        let snap = live.undo().unwrap();
        assert_eq!(snap.graph, first);
        assert_eq!(snap.memory, "m1");
        assert!(live.graph.nodes[0].marks.is_empty());
        assert_eq!(replay(&live.steps).nodes, live.graph.nodes);
        assert_eq!(live.redo().unwrap().graph, marked);
        assert_eq!(replay(&live.steps).nodes, live.graph.nodes);
        live.undo();
        live.change_board(with(&live.graph.clone(), "T 「新」"), json!("m3"));
        assert!(!live.can_redo(), "a new change ends what could be redone");
    }

    #[test]
    fn fields_it_does_not_own_survive_a_snapshot() {
        let mut live = loaded(
            json!({ "version": 1, "id": "a-1", "title": "旧", "titled": true, "summary": "摘要", "voices": [1], "future": { "x": 1 },
                    "graph": { "nodes": [{ "id": "n1", "label": "甲", "kind": "topic", "ink": "ink", "by": "A", "aliases": [], "marks": [] }], "edges": [], "groups": [], "nextNode": 2, "nextGroup": 1 },
                    "lines": [{ "text": "甲", "ms": 3, "via": "远程", "audio": { "start": 1 } }], "history": [{ "id": "x", "text": "甲" }] }),
            Origin::Legacy { source_hash: "h".into() },
        );
        assert!(live.read_only.is_none());
        let raw = live.snapshot();
        for key in ["summary", "voices", "future", "titled"] {
            assert!(raw.contains_key(key), "{key}");
        }
        assert_eq!(raw["graph"]["nodes"][0]["by"], "A");
        assert_eq!(raw["lines"][0]["via"], "远程");
        assert_eq!(raw["history"][0]["id"], "x");
        assert_eq!(raw["title"], "旧", "a named meeting keeps its name");
    }

    #[test]
    fn lines_left_waiting_are_marked_when_reopened() {
        let live = loaded(json!({ "id": "a-1", "lines": [{ "text": "没画完" }, { "text": "手动", "manual": true }] }), Origin::Current);
        assert_eq!(live.lines[0]["failed"], true);
        assert_eq!(live.lines[0]["boardFailure"], "cancelled");
        assert!(live.lines[1].get("failed").is_none());
    }

    #[test]
    fn older_meetings_without_steps_get_one_so_replay_still_works() {
        let live = loaded(
            json!({ "id": "a-1", "updatedAt": 9, "graph": { "nodes": [{ "id": "n1", "label": "甲", "kind": "node", "ink": "ink" }], "edges": [], "groups": [], "nextNode": 2, "nextGroup": 1 } }),
            Origin::Current,
        );
        assert_eq!(live.steps.len(), 1);
        assert_eq!(replay(&live.steps).nodes[0].label, "甲");
    }

    #[test]
    fn imported_or_unreadable_or_newer_meetings_are_view_only() {
        let g = json!({ "nodes": [], "edges": [], "groups": [], "nextNode": 1, "nextGroup": 1 });
        let cases = [
            json!({ "id": "a", "graph": g, "pages": [{ "graph": g, "underlay": "asset:x.png" }] }),
            json!({ "id": "a", "graph": { "nodes": [{ "id": "n1", "label": "原文", "kind": "region" }], "edges": [], "groups": [], "nextNode": 2, "nextGroup": 1 } }),
            json!({ "id": "a", "graph": g, "scene": { "elements": [{ "id": "e" }] } }),
            json!({ "id": "a", "version": 3, "graph": g }),
            json!({ "id": "a", "graph": { "nodes": [{ "id": 7 }] } }),
        ];
        for case in cases {
            assert!(loaded(case.clone(), Origin::Current).read_only.is_some(), "{case}");
        }
        assert!(loaded(json!({ "id": "a", "graph": g, "scene": { "elements": [] } }), Origin::Current).read_only.is_none());
    }

    #[test]
    fn pages_switch_without_losing_either_board() {
        let g = |label: &str| json!({ "nodes": [{ "id": "n1", "label": label, "kind": "topic", "ink": "ink" }], "edges": [], "groups": [], "nextNode": 2, "nextGroup": 1 });
        let mut live = loaded(
            json!({ "id": "a", "graph": g("一"), "page": 0, "pages": [{ "graph": g("一"), "thumb": "t1" }, { "graph": g("二"), "thumb": "t2" }] }),
            Origin::Current,
        );
        assert_eq!(live.pages(), 2);
        let changed = with(&live.graph, "T 「新」");
        live.change_board(changed, Value::Null);
        live.go_to_page(1);
        assert_eq!(live.graph.nodes[0].label, "二");
        let raw = live.snapshot();
        assert_eq!(raw["page"], 1);
        assert_eq!(raw["graph"]["nodes"][0]["label"], "二");
        assert_eq!(raw["pages"][0]["graph"]["nodes"].as_array().unwrap().len(), 2, "the first board keeps its edit");
        assert_eq!(raw["pages"][1]["thumb"], "t2");
    }

    #[test]
    fn a_saved_meeting_reopens_identically() {
        let path = std::env::temp_dir().join(format!("boardcast-live-{}", std::process::id()));
        let store = Store::new(path.clone());
        let mut live = Live::new();
        live.add_line(None, "用户调网关", None);
        let g = with(&live.graph, "E 「用户」 -> 「网关」\nM 「网关」 星标");
        live.model_drew(&[(None, "用户调网关".into())], g, json!({ "regionAt": [["n", { "x": 1, "y": 2 }]] }), "ops", 5);
        let id = live.id().to_string();
        let revision = store.save(&id, live.snapshot(), &live.origin.clone(), live.revision).unwrap();
        live.saved(revision);
        let again = Live::from_loaded(store.load(&id).unwrap().unwrap());
        assert_eq!(again.graph, live.graph);
        assert_eq!(again.memory, live.memory);
        assert_eq!(again.lines, live.lines);
        assert_eq!(again.history, live.history);
        assert_eq!(again.steps, live.steps);
        assert_eq!(again.title(), live.title());
        assert_eq!(again.revision, 1);
        let _ = std::fs::remove_dir_all(path);
    }
}
