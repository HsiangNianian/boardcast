//! The meeting a window has open: the board, what was said, undo, and the replay record.
//! It keeps the file's raw JSON and overwrites only the fields it owns.

use crate::store::{new_id, Loaded, Origin};
use boardcast_board::graph::{Graph, Kind};
use boardcast_board::speakers::{identify_sentence, raised_by, rename_by, rename_by_in_steps, rename_voice, Voice};
use boardcast_board::steps::{diff, ink_only, replay};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub const UNTITLED: &str = "Untitled meeting";
const UNDO_LIMIT: usize = 100;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

/// The undo stacks, ported from the earlier app's edit.ts.
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

    fn each_mut(&mut self, mut change: impl FnMut(&mut T)) {
        self.past.iter_mut().chain(&mut self.future).for_each(&mut change);
    }
}

/// Why a meeting opens without editing. Its file is left exactly as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewOnly {
    /// The board could not be read.
    Unreadable,
    /// Saved by a newer version.
    NewerFormat,
    /// Free-edit content, which this version cannot show or edit.
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
    voices: Vec<Voice>,
    /// A file's voices stay as written unless this meeting changed them.
    voices_changed: bool,
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
        let imported = pages.is_some_and(|p| p.iter().any(|page| scene_has_content(page.get("scene")))) || scene_has_content(raw.get("scene"));
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
            voices: raw.get("voices").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
            voices_changed: false,
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

    /// A new board from step `at` of this one, placed as `memory` says. The original stays as it was.
    pub fn fork_page(&mut self, at: usize, memory: Value, note: &str) {
        self.store_page();
        let at = at.min(self.steps.len());
        if self.raw.get("pages").and_then(Value::as_array).is_none_or(Vec::is_empty) {
            self.raw.insert("pages".into(), json!([{ "graph": self.graph, "layout": self.memory, "steps": self.steps }]));
        }
        let steps: Vec<Value> = self.steps[..at].to_vec();
        let graph = replay(&steps);
        let from = self.page();
        let pages = self.raw.get_mut("pages").and_then(Value::as_array_mut).expect("made above");
        let underlay = pages.get(from).and_then(|p| p.get("underlay")).cloned();
        let mut entry = json!({ "graph": graph, "layout": memory, "steps": steps });
        if let Some(underlay) = underlay {
            entry["underlay"] = underlay;
        }
        pages.push(entry);
        let to = pages.len() - 1;
        self.raw.insert("page".into(), json!(to));
        self.recorded = graph.clone();
        self.graph = graph;
        self.memory = memory;
        self.steps = steps;
        self.undo = History::default();
        self.add_note(note);
        self.touch();
    }

    /// Carry a topic, with everything under it, over to a new board and show that board: the talk moved on
    /// to it. On the board it leaves, its going can be undone and is replayed like any change.
    pub fn split_topic(&mut self, topic: &str, note: &str) -> bool {
        let mut moved: HashSet<String> = [topic.to_string()].into();
        loop {
            let more: Vec<String> = self.graph.nodes.iter().filter(|n| n.parent.as_ref().is_some_and(|p| moved.contains(p)) && !moved.contains(&n.id)).map(|n| n.id.clone()).collect();
            if more.is_empty() {
                break;
            }
            moved.extend(more);
        }
        if !self.graph.nodes.iter().any(|n| n.id == topic) || moved.len() == self.graph.nodes.len() {
            return false;
        }
        let mut left = self.graph.clone();
        left.nodes.retain(|n| !moved.contains(&n.id));
        left.edges.retain(|e| !moved.contains(&e.from) && !moved.contains(&e.to));
        let mut taken = self.graph.clone();
        taken.nodes.retain(|n| moved.contains(&n.id));
        taken.edges.retain(|e| moved.contains(&e.from) && moved.contains(&e.to));
        taken.groups.clear();
        taken.extra.remove("underlay");
        let memory = self.memory.clone();
        self.change_board(left, memory);
        self.store_page();
        if self.raw.get("pages").and_then(Value::as_array).is_none_or(Vec::is_empty) {
            self.raw.insert("pages".into(), json!([{ "graph": self.graph, "layout": self.memory, "steps": self.steps }]));
        }
        let shown = ink_only(&taken);
        let steps: Vec<Value> = diff(&Graph::default(), &shown).map(|d| json!({ "t": now_ms(), "line": self.lines.len(), "d": d })).into_iter().collect();
        let pages = self.raw.get_mut("pages").and_then(Value::as_array_mut).expect("made above");
        pages.push(json!({ "graph": taken, "layout": Value::Null, "steps": steps }));
        let to = pages.len() - 1;
        self.raw.insert("page".into(), json!(to));
        self.recorded = shown;
        self.graph = taken;
        self.memory = Value::Null;
        self.steps = steps;
        self.undo = History::default();
        self.add_note(note);
        self.touch();
        true
    }

    /// Add imported pages, each a board and how to find its picture, and show the first. They take the
    /// place of a board with nothing on it, and otherwise go after the others.
    pub fn add_pages(&mut self, added: Vec<(Graph, String)>, note: &str) {
        if added.is_empty() {
            return;
        }
        self.store_page();
        if self.raw.get("pages").and_then(Value::as_array).is_none_or(Vec::is_empty) {
            self.raw.insert("pages".into(), json!([{ "graph": self.graph, "layout": self.memory, "steps": self.steps }]));
        }
        let current = self.page();
        let blank = self.graph.underlay().is_none() && !self.graph.nodes.iter().any(|n| n.ink == "ink");
        let pages = self.raw.get_mut("pages").and_then(Value::as_array_mut).expect("made above");
        let at = if blank { current } else { pages.len() };
        if blank {
            pages.remove(current);
        }
        let first = added[0].0.clone();
        let entries = added.into_iter().map(|(graph, underlay)| json!({ "graph": graph, "underlay": underlay }));
        pages.splice(at..at, entries);
        self.raw.insert("page".into(), json!(at));
        self.graph = first;
        self.memory = Value::Null;
        self.steps = vec![];
        self.recorded = Graph::default();
        self.undo = History::default();
        self.add_note(note);
        // A meeting already under way keeps the name its own board gave it.
        if blank {
            self.refresh_title();
        }
        self.touch();
    }

    /// Name the meeting, so its board's contents no longer do.
    pub fn set_title(&mut self, title: &str) {
        self.raw.insert("title".into(), json!(title));
        self.raw.insert("titled".into(), json!(true));
        self.touch();
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

    /// The last line asked to turn to another board rather than to draw: it is done, and says where it
    /// turned, so a replay can follow.
    pub fn turned_to(&mut self, page: usize) {
        if let Some(line) = self.lines.last_mut() {
            line["ms"] = json!(0);
            line["turnedTo"] = json!(page);
        }
        self.touch();
    }

    /// Whose voice this is, joining the ones this meeting has heard so that it keeps its name. `None` if
    /// the sentence is too short to name someone new.
    pub fn identify_voice(&mut self, voiceprint: &[f32], seconds: f64) -> Option<String> {
        let print: Vec<f64> = voiceprint.iter().map(|x| f64::from(*x)).collect();
        let taken: Vec<&str> = self.lines.iter().filter_map(|l| l["speaker"].as_str()).collect();
        let name = identify_sentence(&mut self.voices, &print, seconds, &taken)?;
        self.voices_changed = true;
        self.touch();
        Some(name)
    }

    /// Everyone in this meeting, in the order they first spoke.
    pub fn speakers(&self) -> Vec<String> {
        let mut names: Vec<String> = vec![];
        let spoken = self.lines.iter().filter_map(|l| l["speaker"].as_str());
        for name in spoken.chain(self.voices.iter().map(|v| v.name.as_str())) {
            if !name.is_empty() && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names
    }

    /// What renaming a speaker touches: lines of the transcript, things on boards, and how many boards.
    pub fn speaker_impact(&self, who: &str) -> (usize, usize, usize) {
        let lines = self.lines.iter().filter(|l| l["speaker"] == who).count();
        let raised: Vec<usize> = self.boards().iter().map(|graph| raised_by(graph, who)).collect();
        (lines, raised.iter().sum(), raised.iter().filter(|n| **n > 0).count())
    }

    /// Give a speaker a new name everywhere it is written. Taking a name someone has joins the two.
    pub fn rename_speaker(&mut self, from: &str, to: &str) {
        if from == to {
            return;
        }
        for said in self.lines.iter_mut().chain(&mut self.history) {
            if said["speaker"] == from {
                said["speaker"] = json!(to);
            }
        }
        rename_voice(&mut self.voices, from, to);
        self.voices_changed = true;
        rename_by(&mut self.graph, from, to);
        rename_by(&mut self.recorded, from, to);
        rename_by_in_steps(&mut self.steps, from, to);
        self.undo.each_mut(|snap| rename_by(&mut snap.graph, from, to));
        // The boards not on screen stay in the file's own form until they are opened.
        if let Some(pages) = self.raw.get_mut("pages").and_then(Value::as_array_mut) {
            for page in pages {
                for board in ["graph", "base"] {
                    for part in ["nodes", "edges"] {
                        let Some(items) = page.get_mut(board).and_then(|b| b.get_mut(part)).and_then(Value::as_array_mut) else { continue };
                        for item in items.iter_mut().filter(|item| item["by"] == from) {
                            item["by"] = json!(to);
                        }
                    }
                }
                if let Some(steps) = page.get_mut("steps").and_then(Value::as_array_mut) {
                    rename_by_in_steps(steps, from, to);
                }
            }
        }
        self.touch();
    }

    /// Try a line the model never drew again: it goes back to waiting, and out of the context it was
    /// added to when it failed. Gives back who said what, to send on.
    pub fn retry_line(&mut self, index: usize) -> Option<(Option<String>, String)> {
        let line = self.lines.get_mut(index).filter(|l| l["failed"].as_bool().unwrap_or(false))?;
        let was_remembered = line["boardFailure"] == "request";
        let fields = line.as_object_mut()?;
        for field in ["ms", "failed", "boardFailure", "ops"] {
            fields.remove(field);
        }
        let text = line["text"].as_str()?.to_string();
        let speaker = line["speaker"].as_str().map(str::to_string);
        if was_remembered {
            if let Some(at) = self.history.iter().rposition(|said| said["text"] == text.as_str()) {
                self.history.remove(at);
            }
        }
        self.touch();
        Some((speaker, text))
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

    /// The record of every board, in the order of `board_pages`.
    pub fn board_steps(&self) -> Vec<Vec<Value>> {
        let Some(pages) = self.raw.get("pages").and_then(Value::as_array).filter(|p| !p.is_empty()) else {
            return vec![self.steps.clone()];
        };
        let at = self.page();
        pages.iter().enumerate().map(|(i, p)| if i == at { self.steps.clone() } else { array(p.as_object().unwrap_or(&Map::new()), "steps") }).collect()
    }

    /// How each board refers to its imported page's image, in the order of `board_pages`.
    pub fn board_images(&self) -> Vec<Option<String>> {
        let Some(pages) = self.raw.get("pages").and_then(Value::as_array).filter(|p| !p.is_empty()) else {
            return vec![None];
        };
        pages.iter().map(|p| p["underlay"].as_str().map(str::to_string)).collect()
    }

    /// When the meeting began, in milliseconds.
    pub fn created_at(&self) -> Option<f64> {
        self.raw.get("createdAt").and_then(Value::as_f64)
    }

    /// How long the microphone has been on for this meeting, in milliseconds.
    pub fn recorded_ms(&self) -> f64 {
        self.raw.get("recordedMs").and_then(Value::as_f64).unwrap_or(0.0)
    }

    /// Count another stretch of listening towards the meeting.
    pub fn add_recorded(&mut self, ms: f64) {
        let total = self.recorded_ms() + ms.max(0.0);
        self.raw.insert("recordedMs".into(), json!(total.round()));
        self.touch();
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
        if self.voices_changed {
            out.insert("voices".into(), json!(self.voices));
        }
        out
    }

    /// The saved copy has this revision; a later save builds on it.
    pub fn saved(&mut self, revision: u64) {
        // The store records this on the first save, and every later save must keep it.
        if let Origin::Legacy { source_hash } = &self.origin {
            self.raw.entry("migration").or_insert_with(|| json!({ "sourceHash": source_hash }));
        }
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
    fn a_topic_moves_to_a_board_of_its_own() {
        let mut live = Live::new();
        let mut g = Graph::default();
        g.apply(&boardcast_board::graph::parse_ops("T 「登录」\nL 「登录」 「短信验证」\nT 「支付」\nL 「支付」 「微信支付」\nL 「微信支付」 「回调」\nE 「短信验证」 -> 「微信支付」"));
        live.change_board(g, Value::Null);
        let pay = live.graph.nodes.iter().find(|n| n.label == "支付").unwrap().id.clone();
        assert!(live.split_topic(&pay, "新话题"));
        assert_eq!((live.pages(), live.page()), (2, 1));
        let labels = |g: &Graph| g.nodes.iter().map(|n| n.label.clone()).collect::<Vec<_>>();
        assert_eq!(labels(&live.graph), ["支付", "微信支付", "回调"]);
        assert!(live.graph.edges.is_empty(), "a link to what stayed behind is not carried");
        assert!(!live.steps.is_empty(), "the new board replays from its first stroke");
        live.go_to_page(0);
        assert_eq!(labels(&live.graph), ["登录", "短信验证"]);
        assert!(live.graph.edges.is_empty());
        // Nothing to split off, or everything.
        let login = live.graph.nodes[0].id.clone();
        assert!(!live.split_topic(&login, "x"));
        assert!(!live.split_topic("n99", "x"));
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
    fn forking_keeps_the_original_and_starts_from_that_moment() {
        let mut live = Live::new();
        let one = with(&live.graph, "T 「一」");
        live.change_board(one.clone(), json!({ "n": 1 }));
        let two = with(&one, "T 「二」");
        live.change_board(two.clone(), json!({ "n": 2 }));
        assert_eq!(live.steps.len(), 2);
        live.fork_page(1, json!({ "n": 9 }), "另起一板");
        assert_eq!((live.pages(), live.page()), (2, 1));
        assert_eq!(live.graph.nodes, one.nodes);
        assert_eq!(live.memory, json!({ "n": 9 }));
        assert_eq!(live.steps.len(), 1);
        assert!(!live.can_undo());
        assert_eq!(live.lines.last().unwrap()["manual"], true);
        assert_eq!(live.lines.last().unwrap()["page"], 1);
        let boards = live.board_pages();
        assert_eq!(boards[0].0.nodes, two.nodes, "the original is untouched");
        assert_eq!(boards[0].1, json!({ "n": 2 }));
        let raw = live.snapshot();
        assert_eq!(raw["pages"][0]["steps"].as_array().unwrap().len(), 2);
        assert_eq!(replay(raw["steps"].as_array().unwrap()).nodes, one.nodes);
        live.go_to_page(0);
        assert_eq!(live.graph.nodes, two.nodes);
        assert_eq!(live.steps.len(), 2);
    }

    #[test]
    fn imported_pages_can_be_edited_and_free_edit_scenes_cannot() {
        let page = json!({ "nodes": [{ "id": "n1", "label": "原文", "aliases": [], "kind": "region", "marks": [], "ink": "ink", "at": { "x": 1, "y": 2, "w": 3, "h": 4 } }], "edges": [], "groups": [], "nextNode": 2, "nextGroup": 1, "underlay": { "w": 900, "h": 1200 } });
        let imported = loaded(json!({ "id": "m", "graph": page, "page": 0, "pages": [{ "graph": page, "underlay": "asset:abc.jpg" }] }), Origin::Current);
        assert_eq!(imported.read_only, None);
        assert_eq!(imported.board_images(), [Some("asset:abc.jpg".to_string())]);
        let scene = json!({ "elements": [{ "id": "a" }], "files": {} });
        let free = loaded(json!({ "id": "m", "graph": page, "page": 0, "pages": [{ "graph": page, "scene": scene }] }), Origin::Current);
        assert_eq!(free.read_only, Some(ViewOnly::ImportedContent));
    }

    #[test]
    fn saving_a_migrated_meeting_again_keeps_its_migration_record() {
        let dir = std::env::temp_dir().join(format!("boardcast-migration-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("meetings")).unwrap();
        let text = json!({ "id": "m1", "title": "旧" }).to_string();
        std::fs::write(dir.join("meetings/m1.json"), &text).unwrap();
        let store = Store::new(dir.clone());
        let mut live = Live::from_loaded(store.load("m1").unwrap().unwrap());
        for _ in 0..3 {
            let revision = store.save("m1", live.snapshot(), &live.origin.clone(), live.revision).unwrap();
            live.saved(revision);
            let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("meetings/v2/m1.json")).unwrap()).unwrap();
            assert_eq!(on_disk["migration"]["sourceHash"], Store::source_hash(&text));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn page_board(no: u32) -> Graph {
        serde_json::from_value(json!({ "nodes": [{ "id": "n1", "label": format!("第 {no} 页"), "aliases": [], "kind": "region", "marks": [], "ink": "ink", "at": { "x": 1, "y": 2, "w": 3, "h": 4 } }], "edges": [], "groups": [], "nextNode": 2, "nextGroup": 1, "underlay": { "w": 900, "h": 1200, "no": no } })).unwrap()
    }

    #[test]
    fn imported_pages_replace_an_empty_board() {
        let mut live = Live::new();
        live.add_pages(vec![(page_board(1), "asset:a.jpg".into()), (page_board(2), "asset:b.jpg".into())], "导入了");
        assert_eq!((live.pages(), live.page()), (2, 0));
        assert_eq!(live.graph.nodes[0].label, "第 1 页");
        assert_eq!(live.board_images(), [Some("asset:a.jpg".to_string()), Some("asset:b.jpg".to_string())]);
        assert_eq!(live.lines[0]["manual"], true);
        assert_eq!(live.title(), "第 1 页");
        assert!(live.has_content());
        let raw = live.snapshot();
        assert_eq!(raw["pages"][1]["graph"]["underlay"]["no"], 2);
        live.go_to_page(1);
        assert_eq!(live.graph.nodes[0].label, "第 2 页");
    }

    #[test]
    fn imported_pages_go_after_a_board_in_use_and_leave_it_alone() {
        let mut live = Live::new();
        let drawn = with(&live.graph, "T 「已有内容」");
        live.change_board(drawn.clone(), json!({ "n": 1 }));
        live.add_pages(vec![(page_board(1), "asset:a.jpg".into())], "导入了");
        assert_eq!((live.pages(), live.page()), (2, 1));
        assert_eq!(live.lines.last().unwrap()["page"], 1);
        assert!(!live.can_undo(), "an import is not undone with the board's edits");
        assert!(live.steps.is_empty());
        let boards = live.board_pages();
        assert_eq!(boards[0].0.nodes, drawn.nodes);
        assert_eq!(boards[0].1, json!({ "n": 1 }));
        assert_eq!(live.board_images(), [None, Some("asset:a.jpg".to_string())]);
        assert_eq!(live.title(), "已有内容", "the first board still names it");
        // Marking the page records a step that holds the whole page, so a replay can show it.
        let marked = with(&live.graph, "M n1 高亮");
        live.change_board(marked.clone(), Value::Null);
        assert_eq!(live.steps.len(), 1);
        assert_eq!(replay(&live.steps).nodes, marked.nodes);
        assert_eq!(replay(&live.steps).underlay(), Some((900.0, 1200.0)));
    }

    fn said_by(graph: &Graph) -> Vec<Option<&str>> {
        graph.nodes.iter().map(|n| boardcast_board::speakers::by_of_node(n)).collect()
    }

    fn board_by(who: &str) -> Graph {
        let mut graph = page_board(1);
        graph.nodes[0].extra.insert("by".into(), json!(who));
        graph.nodes[0].ink = "ink".into();
        graph
    }

    #[test]
    fn the_same_voice_keeps_its_name_and_new_ones_avoid_names_in_use() {
        let mut live = Live::new();
        live.add_line(Some("A"), "手写的名字", None);
        let (one, other) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_eq!(live.identify_voice(&one, 3.0).unwrap(), "B");
        assert_eq!(live.identify_voice(&other, 3.0).unwrap(), "C");
        assert_eq!(live.identify_voice(&one, 3.0).unwrap(), "B");
        assert_eq!(live.speakers(), ["A", "B", "C"]);
    }

    #[test]
    fn a_new_name_reaches_every_page_step_and_undo_and_stays_with_the_voice() {
        let mut live = Live::new();
        let print = [1.0, 0.0];
        let name = live.identify_voice(&print, 3.0).unwrap();
        live.add_line(Some(&name), "网关", None);
        live.model_drew(&[(Some(name.clone()), "网关".into())], board_by(&name), Value::Null, "", 1);
        live.add_pages(vec![(board_by("别人"), "asset:a.jpg".into())], "导入了");
        // On this page the old name is only in the replay record and the undo stack.
        live.change_board(board_by(&name), Value::Null);
        live.change_board(board_by("别人"), Value::Null);
        assert_eq!(live.page(), 1);
        assert_eq!(live.speaker_impact(&name), (1, 1, 1));

        live.rename_speaker(&name, "李工");

        assert_eq!(live.lines[0]["speaker"], "李工");
        assert_eq!(live.history[0]["speaker"], "李工");
        assert_eq!(live.speaker_impact("李工"), (1, 1, 1));
        assert_eq!(live.speaker_impact(&name), (0, 0, 0));
        let written = serde_json::to_string(&live.snapshot()).unwrap();
        assert!(!written.contains(&format!("\"by\":\"{name}\"")), "{written}");
        assert!(written.contains("\"by\":\"别人\""));
        let snap = live.undo().unwrap();
        assert_eq!(said_by(&snap.graph), [Some("李工")]);
        live.go_to_page(0);
        assert_eq!(said_by(&live.graph), [Some("李工")]);
        // The same voice now answers to the new name.
        assert_eq!(live.identify_voice(&print, 3.0).unwrap(), "李工");
    }

    #[test]
    fn taking_a_name_in_use_joins_the_two_voices() {
        let mut live = Live::new();
        let (a, b) = (live.identify_voice(&[1.0, 0.0], 3.0).unwrap(), live.identify_voice(&[0.0, 1.0], 3.0).unwrap());
        live.add_line(Some(&a), "一", None);
        live.add_line(Some(&b), "二", None);
        live.rename_speaker(&a, &b);
        assert_eq!(live.speakers(), [b.as_str()]);
        assert_eq!(live.speaker_impact(&b).0, 2);
        assert_eq!(live.identify_voice(&[1.0, 1.0], 3.0).unwrap(), b);
    }

    #[test]
    fn voices_are_kept_in_the_file_and_left_as_written_until_changed() {
        let mut live = Live::new();
        live.identify_voice(&[1.0, 0.0], 3.0).unwrap();
        let written = live.snapshot();
        assert_eq!(written["voices"][0]["name"], "A");
        let mut again = Live::build(written, Origin::Current, 1);
        assert_eq!(again.identify_voice(&[1.0, 0.0], 3.0).unwrap(), "A");
        let strange = Live::build(json!({ "id": "x", "voices": [1] }).as_object().unwrap().clone(), Origin::Current, 1).snapshot();
        assert_eq!(strange["voices"], json!([1]));
    }

    #[test]
    fn a_line_that_was_not_drawn_can_be_tried_again() {
        let mut live = Live::new();
        live.add_line(Some("A"), "网关", None);
        live.add_line(None, "还在等", None);
        live.model_failed(&[(Some("A".into()), "网关".into())]);
        assert_eq!(live.history.len(), 1);
        assert_eq!(live.retry_line(1), None, "only a failed line");
        assert_eq!(live.retry_line(0), Some((Some("A".into()), "网关".into())));
        assert!(live.lines[0]["ms"].is_null() && live.lines[0]["failed"].is_null());
        assert!(live.history.is_empty());
        // Back to waiting, so the reply settles it as usual.
        live.model_drew(&[(Some("A".into()), "网关".into())], Graph::default(), Value::Null, "", 5);
        assert_eq!(live.lines[0]["ms"], 5);
        assert_eq!(live.history.len(), 1);
    }

    #[test]
    fn a_line_cut_off_by_closing_is_reopened_for_another_try() {
        let mut live = Live::new();
        live.add_line(None, "没等到", None);
        let mut again = Live::build(live.snapshot(), Origin::Current, 1);
        assert_eq!(again.lines[0]["boardFailure"], "cancelled");
        assert!(again.retry_line(0).is_some());
        assert!(again.history.is_empty());
    }

    #[test]
    fn a_given_title_stays() {
        let mut live = Live::new();
        live.set_title("论文");
        live.add_pages(vec![(page_board(1), "asset:a.jpg".into())], "导入了");
        assert_eq!(live.title(), "论文");
    }

    #[test]
    fn listening_adds_up_and_is_kept() {
        let mut live = Live::new();
        live.add_recorded(1500.0);
        live.add_recorded(2500.4);
        let again = loaded(json!(live.snapshot()), Origin::Current);
        assert_eq!(again.recorded_ms(), 4000.0);
    }

    #[test]
    fn a_fork_keeps_the_page_it_was_drawn_on() {
        let page = with(&Graph::default(), "T 「一」");
        let mut live = loaded(json!({ "id": "m", "graph": page, "page": 0, "pages": [{ "graph": page, "underlay": "asset:abc.jpg" }] }), Origin::Current);
        let two = with(&page, "T 「二」");
        live.change_board(two, Value::Null);
        live.fork_page(1, Value::Null, "另起");
        assert_eq!(live.board_images(), [Some("asset:abc.jpg".to_string()), Some("asset:abc.jpg".to_string())]);
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
    fn free_edit_or_unreadable_or_newer_meetings_are_view_only() {
        let g = json!({ "nodes": [], "edges": [], "groups": [], "nextNode": 1, "nextGroup": 1 });
        let cases = [
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


