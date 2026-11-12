//! The app's board in a web page. It replays the recorded sample meetings: each line is said, the model's
//! recorded reply is applied, and the board is laid out, composed and inked by the same code the app runs,
//! stroke by stroke. A sample with a recording plays along with it, each line said when it was said.
//! Only the window around it is drawn here. landing/demo/index.html is the page.

use boardcast_board::board_layout::{Bounds, StableLayout};
use boardcast_board::compose::{compose, Style};
use boardcast_board::glyphs::{Glyphs, HanziSource, Pt};
use boardcast_board::graph::{parse_ops, Graph, Kind};
use boardcast_board::pen::{outline, Inked, Rng};
use boardcast_board::sugiyama::first_diagram;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, Element, HtmlAudioElement};

#[derive(Deserialize)]
struct Sample {
    id: String,
    name: String,
    lines: Vec<SampleLine>,
    /// The meeting's recording, in the site's media folder, for a sample taken from a real one.
    #[serde(default)]
    audio: Option<String>,
    /// Whose recording it is, shown while it plays.
    #[serde(default)]
    credit: Option<String>,
}

#[derive(Deserialize)]
struct SampleLine {
    speaker: String,
    text: String,
    /// How long the model took to answer when the sample was recorded.
    ms: f64,
    ops: Option<String>,
    /// When the line is said in the recording, in seconds.
    #[serde(default)]
    start: Option<f64>,
    #[serde(default)]
    end: Option<f64>,
}

/// Recorded English meetings for the web demo.
fn samples() -> Vec<Sample> {
    [
        include_str!("../../../assets/demos/en-office-plan.json"),
        include_str!("../../../assets/demos/en-release-priorities.json"),
        include_str!("../../../assets/demos/en-auth-correction.json"),
        include_str!("../../../assets/demos/en-upload-branch.json"),
        include_str!("../../../assets/demos/en-platform-groups.json"),
        include_str!("../../../assets/demos/en-owner-handoff.json"),
    ]
    .iter()
    .map(|s| serde_json::from_str(s).expect("a sample meeting"))
    .collect()
}

/// Short names for the sample list, in the samples' order.
const TITLES: [&str; 6] = ["Office plan · recorded", "Release checklist", "Sign-in flow", "Upload pipeline", "Service groups", "Owner handoff"];

/// CSS recolors existing strokes without clearing or restarting the board.
fn ink_color(rgb: u32) -> String {
    match rgb {
        0x1e_1e_1e => "var(--board-ink, #ecece6)".into(),
        0xe0_31_31 => "var(--board-red, #ff5a36)".into(),
        0x19_71_c2 => "var(--board-blue, #9fb4ff)".into(),
        other => format!("#{other:06x}"),
    }
}

/// Board units of ink per millisecond, as quick as a hand that knows what it is writing.
const PEN_SPEED: f32 = 1.4;
/// Room around the ink, and the least the canvas shows. Compact canvases show less to keep ink legible.
const MARGIN: f64 = 80.0;
const MIN_VIEW: (f64, f64) = (900.0, 560.0);
/// The most of the board shown at once; past this the view follows the newest ink, so writing stays legible.
const MAX_VIEW_WIDTH: f64 = 1500.0;
/// How long after the reader last moved the board the view goes back to the talk.
const READ_HOLD_MS: f64 = 5000.0;
/// How near and how far the reader can zoom, as the width of board shown.
const ZOOM_WIDTH: (f64, f64) = (240.0, 8000.0);

fn win() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn doc() -> Document {
    win().document().expect("a document")
}

fn q(sel: &str) -> Element {
    doc().query_selector(sel).ok().flatten().unwrap_or_else(|| panic!("no {sel} on the page"))
}

fn el(tag: &str, class: &str, text: &str) -> Element {
    let e = doc().create_element(tag).expect("an element");
    if !class.is_empty() {
        e.set_class_name(class);
    }
    if !text.is_empty() {
        e.set_text_content(Some(text));
    }
    e
}

fn svg_el(name: &str, attrs: &[(&str, &str)]) -> Element {
    let e = doc().create_element_ns(Some("http://www.w3.org/2000/svg"), name).expect("an SVG element");
    for (k, v) in attrs {
        let _ = e.set_attribute(k, v);
    }
    e
}

fn reduced() -> bool {
    win().match_media("(prefers-reduced-motion: reduce)").ok().flatten().is_some_and(|m| m.matches())
}

async fn sleep(ms: f64) {
    let promise = js_sys::Promise::new(&mut |done, _| {
        let _ = win().set_timeout_with_callback_and_timeout_and_arguments_0(&done, ms as i32);
    });
    let _ = JsFuture::from(promise).await;
}

async fn frame() -> f64 {
    let promise = js_sys::Promise::new(&mut |done, _| {
        let _ = win().request_animation_frame(&done);
    });
    JsFuture::from(promise).await.ok().and_then(|t| t.as_f64()).unwrap_or(0.0)
}

fn path_of(points: &[Pt]) -> String {
    let mut d = String::with_capacity(points.len() * 12);
    for (i, p) in points.iter().enumerate() {
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&format!("{:.1} {:.1}", p[0], p[1]));
    }
    d.push('Z');
    d
}

/// A stroke on the page: its shape so far is redrawn while the pen moves along it.
struct Drawing {
    node: Element,
    inked: Inked,
}

struct Board {
    svg: Element,
    under: Element,
    ink: Element,
    glyphs: Glyphs,
    layout: StableLayout,
    graph: Graph,
    shown: HashSet<String>,
    /// Where each piece on the board starts, by what it is rather than where, to tell a move from new ink.
    placed: HashMap<String, [f32; 2]>,
    view: [f64; 4],
    /// Whether the reader moved the board themselves, when they last did, and where the talk's ink is.
    reading: bool,
    touched: f64,
    latest: [f64; 4],
    /// Whether the reader is holding the board down right now.
    holding: bool,
    back: Element,
}

impl Board {
    fn new() -> Self {
        let root = q("#board");
        let (under, ink) = (svg_el("g", &[]), svg_el("g", &[]));
        let _ = root.append_child(&under);
        let _ = root.append_child(&ink);
        // Only the characters the Chinese samples use, from hanzi-writer-data (Arphic Public License):
        // `boardcast-tools pack-glyphs assets/hanzi crates/demo/zh-glyphs.pack --only-from <the samples>`.
        let glyphs = Glyphs::load(HanziSource::PackBytes(include_bytes!("../zh-glyphs.pack"))).expect("the samples' strokes");
        let view = [0.0, 0.0, MIN_VIEW.0, MIN_VIEW.1];
        Self { svg: root, under, ink, glyphs, layout: StableLayout::default(), graph: Graph::default(), shown: HashSet::new(), placed: HashMap::new(), view, reading: false, touched: 0.0, latest: view, holding: false, back: q("#latest") }
    }

    fn clear(&mut self) {
        self.set_reading(false);
        self.under.set_text_content(None);
        self.ink.set_text_content(None);
        self.layout = StableLayout::default();
        self.graph = Graph::default();
        self.shown.clear();
        self.placed.clear();
    }

    /// Apply a reply and lay the board out again. What was already on the board is redrawn where it now
    /// sits; what is new comes back to be written.
    fn apply(&mut self, ops: &str) -> (Vec<Drawing>, Bounds) {
        self.graph.apply(&parse_ops(ops));
        let laid = self.layout.layout(&self.graph, &mut first_diagram);
        let (pieces, _) = compose(&self.graph, &laid, &mut self.glyphs);
        self.under.set_text_content(None);
        self.ink.set_text_content(None);
        let mut fresh = vec![];
        let mut keys = HashSet::new();
        let mut placed = HashMap::new();
        let mut sliding = vec![];
        // Notes and marks can reach past the laid-out boxes, so the view is framed on the ink itself.
        let mut ink = Bounds { ..laid.bounds };
        for piece in &pieces {
            keys.insert(piece.key.clone());
            // Ink that was on the board somewhere else only moved, pushed along by a line written above it:
            // it slides to its new place instead of being written again.
            let before = self.placed.get(&piece.seed).copied();
            let new = !self.shown.contains(&piece.key) && before.is_none();
            // The same seed in the same order gives the same wobble as in the app.
            let mut r = Rng::new(&piece.seed);
            for (n, line) in piece.lines.iter().enumerate() {
                let inked = line.inked(&mut r);
                if n == 0 {
                    if let Some(first) = inked.center.first() {
                        placed.insert(piece.seed.clone(), *first);
                    }
                }
                let pad = f64::from(inked.width);
                for p in &inked.center {
                    let (x, y) = (f64::from(p[0]), f64::from(p[1]));
                    (ink.x0, ink.y0, ink.x1, ink.y1) = (ink.x0.min(x - pad), ink.y0.min(y - pad), ink.x1.max(x + pad), ink.y1.max(y + pad));
                }
                let node = stroke_node(line.style);
                let layer = if line.style.under { &self.under } else { &self.ink };
                let _ = layer.append_child(&node);
                if new {
                    fresh.push(Drawing { node, inked });
                } else {
                    let _ = node.set_attribute("d", &path_of(&outline(&inked, 1.0)));
                    let first = placed.get(&piece.seed).copied();
                    let moved = !self.shown.contains(&piece.key);
                    if let (true, Some(from), Some(to)) = (moved, before, first) {
                        let (dx, dy) = (from[0] - to[0], from[1] - to[1]);
                        let _ = node.set_attribute("style", &format!("transform:translate({dx:.1}px,{dy:.1}px)"));
                        sliding.push(node);
                    }
                }
            }
        }
        // Moved ink starts where it was and eases into place once the page has drawn it there.
        if !sliding.is_empty() && !reduced() {
            wasm_bindgen_futures::spawn_local(async move {
                frame().await;
                frame().await;
                for node in sliding {
                    let _ = node.set_attribute("style", "transition:transform 220ms cubic-bezier(.2,.7,.3,1);transform:none");
                }
            });
        } else {
            for node in sliding {
                let _ = node.remove_attribute("style");
            }
        }
        self.shown = keys;
        self.placed = placed;
        (fresh, ink)
    }

    /// Where the newest strokes are, or the board's bounds when there are none.
    fn near(fresh: &[Drawing], all: &Bounds) -> Bounds {
        let points = fresh.iter().flat_map(|d| d.inked.center.iter());
        let mut b = Bounds { x0: f64::MAX, y0: f64::MAX, x1: f64::MIN, y1: f64::MIN };
        for p in points {
            (b.x0, b.y0, b.x1, b.y1) = (b.x0.min(f64::from(p[0])), b.y0.min(f64::from(p[1])), b.x1.max(f64::from(p[0])), b.y1.max(f64::from(p[1])));
        }
        if b.x0 > b.x1 { Bounds { ..*all } } else { b }
    }

    fn set_reading(&mut self, reading: bool) {
        self.reading = reading;
        let _ = self.back.class_list().toggle_with_force("shown", reading);
    }

    /// The board point under a point of the page, and board units per pixel.
    fn at(&self, x: f64, y: f64) -> ([f64; 2], f64) {
        let r = self.svg.get_bounding_client_rect();
        let v = self.view;
        // The view is fitted inside the element, centred, so one of its sides has room around it.
        let s = (v[2] / r.width().max(1.0)).max(v[3] / r.height().max(1.0));
        let (ox, oy) = ((r.width() * s - v[2]) / 2.0, (r.height() * s - v[3]) / 2.0);
        ([v[0] - ox + (x - r.left()) * s, v[1] - oy + (y - r.top()) * s], s)
    }

    fn set_view(&mut self, v: [f64; 4]) {
        self.view = v;
        let _ = self.svg.set_attribute("viewBox", &format!("{:.1} {:.1} {:.1} {:.1}", v[0], v[1], v[2], v[3]));
    }
}

/// The part of the board to show: all of it if that stays legible, or else the newest ink, inside the board.
fn frame_following(all: &Bounds, newest: &Bounds) -> [f64; 4] {
    let whole = frame_for(all);
    if whole[2] <= MAX_VIEW_WIDTH {
        return whole;
    }
    let (w, h) = (MAX_VIEW_WIDTH, MAX_VIEW_WIDTH * whole[3] / whole[2]);
    let (cx, cy) = ((newest.x0 + newest.x1) / 2.0, (newest.y0 + newest.y1) / 2.0);
    let clamp = |c: f64, half: f64, lo: f64, hi: f64| if hi - lo <= 2.0 * half { (lo + hi) / 2.0 } else { c.clamp(lo + half, hi - half) };
    let x = clamp(cx, w / 2.0, all.x0 - MARGIN, all.x1 + MARGIN) - w / 2.0;
    let y = clamp(cy, h / 2.0, all.y0 - MARGIN, all.y1 + MARGIN) - h / 2.0;
    [x, y, w, h]
}

/// The part of the board to show: all its ink, centred, never closer than a set size, in the canvas's shape.
fn frame_for(b: &Bounds) -> [f64; 4] {
    let (bw, bh, cx, cy) = if b.x1 > b.x0 { (b.x1 - b.x0, b.y1 - b.y0, (b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0) } else { (0.0, 0.0, 0.0, 0.0) };
    let r = q("#board").get_bounding_client_rect();
    let narrow = (r.width() / MIN_VIEW.0).min(r.height() / MIN_VIEW.1).clamp(0.45, 1.0);
    let margin = MARGIN * narrow;
    let (w, h) = ((bw + margin * 2.0).max(MIN_VIEW.0 * narrow), (bh + margin * 2.0).max(MIN_VIEW.1 * narrow));
    let aspect = if r.height() > 0.0 { r.width() / r.height() } else { 1.6 };
    let (w, h) = if w / h > aspect { (w, w / aspect) } else { (h * aspect, h) };
    [cx - w / 2.0, cy - h / 2.0, w, h]
}

fn stroke_node(style: Style) -> Element {
    svg_el("path", &[("fill", &ink_color(style.rgb)), ("fill-opacity", &style.alpha.to_string())])
}

struct Ui {
    title: Element,
    state: Element,
    lines: Element,
    tabs: Vec<Element>,
    listen: Element,
    sound: Element,
    credit: Element,
    audio: HtmlAudioElement,
}

impl Ui {
    fn set_state(&self, s: &str) {
        self.state.set_text_content(Some(&format!("board / {s}")));
        let _ = self.state.class_list().toggle_with_force("hot", s == "drawing");
    }
}

/// What the reader asked for: a sample to play, and whether to keep going.
struct Control {
    wanted: Cell<Option<usize>>,
    paused: Cell<bool>,
    /// Bumped whenever a sample starts, so the one before stops where it is.
    run: Cell<u32>,
    /// Whether the reader turned the recording's sound on. A page may only start sound after a click.
    sound: Cell<bool>,
}

/// Play one sample through. False if the reader started another one meanwhile.
async fn play(sample: &Sample, index: usize, board: &Rc<RefCell<Board>>, ui: &Rc<Ui>, control: &Rc<Control>, me: u32) -> bool {
    let live = || control.run.get() == me;
    board.borrow_mut().clear();
    ui.lines.set_text_content(None);
    ui.title.set_text_content(Some(TITLES[index]));
    let _ = ui.audio.pause();
    let recorded = sample.audio.is_some();
    let _ = ui.sound.class_list().toggle_with_force("shown", recorded);
    let _ = ui.credit.class_list().toggle_with_force("shown", recorded);
    ui.credit.set_text_content(sample.credit.as_deref());
    let _ = ui.listen.class_list().toggle_with_force("paused", control.paused.get());
    q("#listen-label").set_text_content(Some(if control.paused.get() { "resume" } else { "pause" }));
    for (i, t) in ui.tabs.iter().enumerate() {
        let _ = t.class_list().toggle_with_force("on", i == index);
    }
    board.borrow_mut().set_view(frame_for(&Bounds { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 }));
    ui.set_state("listening");
    if let Some(audio) = &sample.audio {
        return play_recorded(sample, audio, board, ui, control, me).await;
    }
    sleep(600.0).await;
    for line in &sample.lines {
        while control.paused.get() && live() {
            sleep(120.0).await;
        }
        if !live() {
            return false;
        }
        // The line comes in as it is recognized.
        let li = el("li", "live", "");
        let _ = li.append_child(&el("b", "who", &line.speaker.chars().take(1).collect::<String>()));
        let body = el("div", "", "");
        let _ = body.append_child(&el("span", "name", &line.speaker));
        let said = el("p", "", "");
        let _ = body.append_child(&said);
        let _ = li.append_child(&body);
        let _ = ui.lines.append_child(&li);
        let chars: Vec<char> = line.text.chars().collect();
        let step = if reduced() { chars.len().max(1) } else { 3 };
        let mut shown = 0;
        while shown < chars.len() {
            if !live() {
                return false;
            }
            shown = (shown + step).min(chars.len());
            said.set_text_content(Some(&chars[..shown].iter().collect::<String>()));
            ui.lines.set_scroll_top(ui.lines.scroll_height());
            sleep(28.0).await;
        }
        let _ = li.class_list().remove_1("live");
        // The model thinks for about as long as it did when the sample was made.
        ui.set_state("thinking");
        sleep(line.ms.clamp(300.0, 1500.0)).await;
        if !live() {
            return false;
        }
        let Some(ops) = &line.ops else {
            ui.set_state("listening");
            continue;
        };
        let (fresh, bounds) = board.borrow_mut().apply(ops);
        if let Some(head) = board.borrow().graph.nodes.iter().find(|n| matches!(n.kind, Kind::Topic)) {
            ui.title.set_text_content(Some(&head.label));
        }
        ui.set_state("drawing");
        let newest = Board::near(&fresh, &bounds);
        follow(board, frame_for(&bounds), &newest, &live).await;
        write(&fresh, &live).await;
        let _ = li.class_list().add_1("drawn");
        ui.set_state("ready");
        sleep(900.0).await;
    }
    live()
}

/// A line in the transcript as it is being said.
fn line_item(ui: &Ui, speaker: &str) -> (Element, Element) {
    let li = el("li", "live", "");
    let _ = li.append_child(&el("b", "who", &speaker.chars().take(1).collect::<String>()));
    let body = el("div", "", "");
    let _ = body.append_child(&el("span", "name", speaker));
    let said = el("p", "", "");
    let _ = body.append_child(&said);
    let _ = li.append_child(&body);
    let _ = ui.lines.append_child(&li);
    (li, said)
}

/// The recording's time, in seconds. It follows the audio while that plays, and keeps going on its own
/// if the audio cannot, so the meeting plays through either way.
struct Clock {
    at: Cell<f64>,
    last: Cell<f64>,
}

impl Clock {
    fn tick(&self, now_ms: f64, audio: &HtmlAudioElement, paused: bool) -> f64 {
        let dt = ((now_ms - self.last.replace(now_ms)) / 1000.0).clamp(0.0, 0.25);
        let playing = !audio.paused() && audio.error().is_none() && audio.ready_state() >= 2;
        if playing {
            self.at.set(audio.current_time());
        } else if !paused {
            self.at.set(self.at.get() + dt);
        }
        self.at.get()
    }
}

/// Play a sample along with its recording: each line typed out while it is said, and the model's reply
/// drawn once it would have come back, one after another, while the talk goes on.
async fn play_recorded(sample: &Sample, audio_name: &str, board: &Rc<RefCell<Board>>, ui: &Rc<Ui>, control: &Rc<Control>, me: u32) -> bool {
    let live = || control.run.get() == me;
    let audio = &ui.audio;
    audio.set_src(&format!("../media/{audio_name}"));
    audio.set_muted(!control.sound.get());
    audio.set_current_time(0.0);
    if !control.paused.get() {
        let _ = audio.play();
    }
    let clock = Clock { at: Cell::new(0.0), last: Cell::new(frame().await) };
    // Replies wait here for the board, which writes them in order while the transcript runs on.
    let queue: Rc<RefCell<VecDeque<(String, Element)>>> = Rc::new(RefCell::new(VecDeque::new()));
    let idle = Rc::new(Cell::new(true));
    let talk_over = Rc::new(Cell::new(false));
    {
        let (queue, idle, talk_over, board, ui, control) = (queue.clone(), idle.clone(), talk_over.clone(), board.clone(), ui.clone(), control.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let live = || control.run.get() == me;
            loop {
                if !live() {
                    return;
                }
                let next = queue.borrow_mut().pop_front();
                let Some((ops, li)) = next else {
                    idle.set(true);
                    if talk_over.get() {
                        return;
                    }
                    frame().await;
                    continue;
                };
                idle.set(false);
                let (fresh, bounds) = board.borrow_mut().apply(&ops);
                if let Some(head) = board.borrow().graph.nodes.iter().find(|n| matches!(n.kind, Kind::Topic)) {
                    ui.title.set_text_content(Some(&head.label));
                }
                ui.set_state("drawing");
                let newest = Board::near(&fresh, &bounds);
                follow(&board, frame_following(&bounds, &newest), &newest, &live).await;
                write(&fresh, &live).await;
                let _ = li.class_list().add_1("drawn");
                ui.set_state("listening");
            }
        });
    }
    let end = sample.lines.iter().filter_map(|l| l.end).fold(0.0, f64::max);
    let mut said: Vec<(Element, Element, usize)> = vec![];
    let mut sent = 0;
    loop {
        let now = frame().await;
        if !live() {
            let _ = audio.pause();
            return false;
        }
        let paused = control.paused.get();
        if paused && !audio.paused() {
            let _ = audio.pause();
        } else if !paused && audio.paused() && audio.error().is_none() && clock.at.get() < end {
            audio.set_current_time(clock.at.get());
            let _ = audio.play();
        }
        audio.set_muted(!control.sound.get());
        let t = clock.tick(now, audio, paused);
        // Lines come in as they are said, and are typed out over the time they take.
        while said.len() < sample.lines.len() && sample.lines[said.len()].start.unwrap_or(0.0) <= t {
            let (li, p) = line_item(ui, &sample.lines[said.len()].speaker);
            said.push((li, p, 0));
            ui.lines.set_scroll_top(ui.lines.scroll_height());
        }
        for (i, (li, p, shown)) in said.iter_mut().enumerate() {
            let line = &sample.lines[i];
            let chars: Vec<char> = line.text.chars().collect();
            if *shown >= chars.len() {
                continue;
            }
            let (start, stop) = (line.start.unwrap_or(0.0), line.end.unwrap_or(0.0));
            let k = if reduced() || stop <= start { 1.0 } else { ((t - start) / (stop - start)).clamp(0.0, 1.0) };
            let n = ((chars.len() as f64 * k).ceil() as usize).min(chars.len());
            if n > *shown {
                *shown = n;
                p.set_text_content(Some(&chars[..n].iter().collect::<String>()));
                ui.lines.set_scroll_top(ui.lines.scroll_height());
            }
            if n == chars.len() {
                let _ = li.class_list().remove_1("live");
            }
        }
        // A reply reaches the board as long after the line as the model took when the sample was made.
        while sent < said.len() {
            let line = &sample.lines[sent];
            if t < line.end.unwrap_or(0.0) + line.ms.clamp(300.0, 1500.0) / 1000.0 {
                break;
            }
            if let Some(ops) = &line.ops {
                queue.borrow_mut().push_back((ops.clone(), said[sent].0.clone()));
            } else {
                let _ = said[sent].0.class_list().add_1("drawn");
            }
            sent += 1;
        }
        if sent == sample.lines.len() {
            talk_over.set(true);
            if idle.get() && queue.borrow().is_empty() {
                let _ = audio.pause();
                return live();
            }
        }
    }
}

/// Take the view to new ink, unless the reader is looking around the board: then only once the ink is out
/// of their sight and they have left the board alone a while.
async fn follow(board: &RefCell<Board>, to: [f64; 4], newest: &Bounds, live: &dyn Fn() -> bool) {
    {
        let mut b = board.borrow_mut();
        b.latest = to;
        if b.reading {
            let v = b.view;
            let inside = newest.x0 >= v[0] && newest.y0 >= v[1] && newest.x1 <= v[0] + v[2] && newest.y1 <= v[1] + v[3];
            if inside || b.holding || now() - b.touched < READ_HOLD_MS {
                return;
            }
            b.set_reading(false);
        }
    }
    ease_view(board, to, live).await;
}

fn now() -> f64 {
    win().performance().map_or(0.0, |p| p.now())
}

/// Move the view to where the new ink goes, quickly. The reader taking hold of the board stops it.
async fn ease_view(board: &RefCell<Board>, to: [f64; 4], live: &dyn Fn() -> bool) {
    let from = board.borrow().view;
    if reduced() {
        board.borrow_mut().set_view(to);
        return;
    }
    let t0 = frame().await;
    loop {
        let t = frame().await;
        let k = ((t - t0) / 450.0).min(1.0);
        let e = 1.0 - (1.0 - k).powi(3);
        if board.borrow().reading {
            break;
        }
        board.borrow_mut().set_view(std::array::from_fn(|i| from[i] + (to[i] - from[i]) * e));
        if k >= 1.0 || !live() {
            break;
        }
    }
}

/// Write new strokes one after another, each as far as the pen has gone.
async fn write(strokes: &[Drawing], live: &dyn Fn() -> bool) {
    if reduced() {
        for s in strokes {
            let _ = s.node.set_attribute("d", &path_of(&outline(&s.inked, 1.0)));
        }
        return;
    }
    let lengths: Vec<f32> = strokes.iter().map(|s| s.inked.length.max(1.0)).collect();
    let total: f32 = lengths.iter().sum();
    // A long reply is written faster, so no line waits more than a few seconds for its ink.
    let speed = PEN_SPEED.max(total / 3500.0);
    let t0 = frame().await;
    let mut done = 0;
    let mut before = 0.0;
    while done < strokes.len() {
        let t = frame().await;
        if !live() {
            return;
        }
        let pen = (t - t0) as f32 * speed;
        // Finish every stroke the pen has passed, then draw the one it is on up to the pen.
        while done < strokes.len() && pen >= before + lengths[done] {
            let _ = strokes[done].node.set_attribute("d", &path_of(&outline(&strokes[done].inked, 1.0)));
            before += lengths[done];
            done += 1;
        }
        if done < strokes.len() {
            let k = ((pen - before) / lengths[done]).clamp(0.0, 1.0);
            let _ = strokes[done].node.set_attribute("d", &path_of(&outline(&strokes[done].inked, k)));
        }
    }
}

/// A drag under way: the pointer, where it went down, and the view then.
type Grip = (i32, f64, f64, [f64; 4]);

fn on(target: &web_sys::EventTarget, event: &str, f: impl FnMut(web_sys::Event) + 'static) {
    let f = Closure::<dyn FnMut(web_sys::Event)>::new(f);
    let _ = target.add_event_listener_with_callback(event, f.as_ref().unchecked_ref());
    f.forget();
}

#[wasm_bindgen(start)]
pub fn start() {
    let embed = win().location().search().unwrap_or_default().contains("embed");
    let _ = doc().document_element().expect("a root").class_list().toggle_with_force("embed", embed);

    let samples = Rc::new(samples());
    let picker = q("#samples");
    let tabs: Vec<Element> = samples
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let b = el("button", "", TITLES[i]);
            let _ = b.set_attribute("title", &s.name);
            let _ = b.set_attribute("data-sample", &s.id);
            let _ = picker.append_child(&b);
            b
        })
        .collect();
    let audio = HtmlAudioElement::new().expect("an audio element");
    audio.set_preload("auto");
    let ui = Rc::new(Ui { title: q("#title"), state: q("#state"), lines: q("#lines"), tabs: tabs.clone(), listen: q("#listen"), sound: q("#sound"), credit: q("#credit"), audio });
    let control = Rc::new(Control { wanted: Cell::new(None), paused: Cell::new(false), run: Cell::new(0), sound: Cell::new(false) });
    {
        let (control, ui) = (control.clone(), ui.clone());
        on(&q("#sound"), "click", move |_| {
            control.sound.set(!control.sound.get());
            ui.audio.set_muted(!control.sound.get());
            // Sound may only start from a click, so this is where a refused start is tried again.
            if control.sound.get() && !control.paused.get() && ui.audio.paused() {
                let _ = ui.audio.play();
            }
            let _ = ui.sound.class_list().toggle_with_force("on", control.sound.get());
            q("#sound-label").set_text_content(Some(if control.sound.get() { "sound on" } else { "sound off" }));
        });
    }
    let board = Rc::new(RefCell::new(Board::new()));

    for (i, b) in tabs.iter().enumerate() {
        let control = control.clone();
        on(b, "click", move |_| {
            control.wanted.set(Some(i));
            control.paused.set(false);
            control.run.set(control.run.get() + 1);
        });
    }
    {
        let (control, ui) = (control.clone(), ui.clone());
        on(&q("#listen"), "click", move |_| {
            control.paused.set(!control.paused.get());
            let _ = ui.listen.class_list().toggle_with_force("paused", control.paused.get());
            q("#listen-label").set_text_content(Some(if control.paused.get() { "resume" } else { "pause" }));
        });
    }
    // The reader can take hold of the board: drag to look around, and the wheel or a pinch to zoom, as in
    // the app. Over the board the wheel is the board's, not the page's.
    {
        let drag: Rc<Cell<Option<Grip>>> = Rc::new(Cell::new(None));
        let svg = q("#board");
        {
            let (board, drag, target) = (board.clone(), drag.clone(), svg.clone());
            on(&svg, "pointerdown", move |e| {
                let Some(e) = e.dyn_ref::<web_sys::PointerEvent>() else { return };
                if e.button() != 0 {
                    return;
                }
                let _ = target.set_pointer_capture(e.pointer_id());
                let _ = target.class_list().add_1("dragging");
                board.borrow_mut().holding = true;
                drag.set(Some((e.pointer_id(), f64::from(e.client_x()), f64::from(e.client_y()), board.borrow().view)));
            });
        }
        {
            let (board, drag) = (board.clone(), drag.clone());
            on(&svg, "pointermove", move |e| {
                let (Some(e), Some((id, x, y, from))) = (e.dyn_ref::<web_sys::PointerEvent>(), drag.get()) else { return };
                if e.pointer_id() != id {
                    return;
                }
                let mut b = board.borrow_mut();
                let (_, s) = b.at(x, y);
                let (dx, dy) = ((f64::from(e.client_x()) - x) * s, (f64::from(e.client_y()) - y) * s);
                if dx.abs() + dy.abs() < 0.5 && !b.reading {
                    return;
                }
                b.set_reading(true);
                b.touched = now();
                b.set_view([from[0] - dx, from[1] - dy, from[2], from[3]]);
            });
        }
        for end in ["pointerup", "pointercancel"] {
            let (drag, target, board) = (drag.clone(), svg.clone(), board.clone());
            on(&svg, end, move |_| {
                drag.set(None);
                let mut b = board.borrow_mut();
                b.holding = false;
                b.touched = now();
                let _ = target.class_list().remove_1("dragging");
            });
        }
        {
            let board = board.clone();
            let wheel = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |e: web_sys::WheelEvent| {
                e.prevent_default();
                let mut b = board.borrow_mut();
                let (p, _) = b.at(f64::from(e.client_x()), f64::from(e.client_y()));
                let v = b.view;
                // The same response as the app's wheel, exponential in the distance scrolled; a pinch comes
                // as a wheel with Ctrl held, in smaller steps. Lines and pages are turned into pixels first.
                let pixels = e.delta_y() * match e.delta_mode() { 1 => 16.0, 2 => 400.0, _ => 1.0 };
                let k = (pixels * if e.ctrl_key() { 0.01 } else { 0.0015 }).exp().clamp(ZOOM_WIDTH.0 / v[2], ZOOM_WIDTH.1 / v[2]);
                b.set_reading(true);
                b.touched = now();
                b.set_view([p[0] - (p[0] - v[0]) * k, p[1] - (p[1] - v[1]) * k, v[2] * k, v[3] * k]);
            });
            let options = web_sys::AddEventListenerOptions::new();
            options.set_passive(false);
            let _ = svg.add_event_listener_with_callback_and_add_event_listener_options("wheel", wheel.as_ref().unchecked_ref(), &options);
            wheel.forget();
        }
        // Left alone a while, the board goes back to where the talk is.
        {
            let board = board.clone();
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    sleep(250.0).await;
                    let back = {
                        let b = board.borrow();
                        b.reading && !b.holding && now() - b.touched >= READ_HOLD_MS
                    };
                    if back {
                        let to = board.borrow().latest;
                        board.borrow_mut().set_reading(false);
                        ease_view(&board, to, &|| true).await;
                    }
                }
            });
        }
        {
            let board = board.clone();
            on(&q("#latest"), "click", move |_| {
                let to = board.borrow().latest;
                board.borrow_mut().set_reading(false);
                let board = board.clone();
                wasm_bindgen_futures::spawn_local(async move { ease_view(&board, to, &|| true).await });
            });
        }
    }
    {
        let control = control.clone();
        on(&q("#replay"), "click", move |_| {
            control.paused.set(false);
            control.run.set(control.run.get() + 1);
        });
    }

    wasm_bindgen_futures::spawn_local(async move {
        let mut index = 0;
        loop {
            let me = control.run.get();
            if let Some(w) = control.wanted.take() {
                index = w;
            }
            let finished = play(&samples[index], index, &board, &ui, &control, me).await;
            if finished {
                // Hold the finished board a moment, then go on to the next sample.
                sleep(3200.0).await;
                if control.run.get() == me {
                    index = (index + 1) % samples.len();
                    control.run.set(me + 1);
                }
            }
        }
    });
}
