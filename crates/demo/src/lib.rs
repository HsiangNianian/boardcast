//! The app's board in a web page. It replays the recorded sample meetings: each line is said, the model's
//! recorded reply is applied, and the board is laid out, composed and inked by the same code the app runs,
//! stroke by stroke. Only the window around it is drawn here. landing/demo/index.html is the page.

use boardcast_board::board_layout::{Bounds, StableLayout};
use boardcast_board::compose::{compose, Style};
use boardcast_board::glyphs::{Glyphs, HanziSource, Pt};
use boardcast_board::graph::{parse_ops, Graph, Kind};
use boardcast_board::pen::{outline, Inked, Rng};
use boardcast_board::sugiyama::first_diagram;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, Element};

#[derive(Deserialize)]
struct Sample {
    id: String,
    name: String,
    lines: Vec<SampleLine>,
}

#[derive(Deserialize)]
struct SampleLine {
    speaker: String,
    text: String,
    /// How long the model took to answer when the sample was recorded.
    ms: f64,
    ops: Option<String>,
}

/// The English samples: their boards need only the Latin strokes built into the board crate.
fn samples() -> Vec<Sample> {
    [
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
const TITLES: [&str; 5] = ["Release checklist", "Sign-in flow", "Upload pipeline", "Service groups", "Owner handoff"];

/// The app's dark board shows strokes, composed in marker colors, in their chalk.
fn chalk(rgb: u32) -> String {
    let c = match rgb {
        0x1e_1e_1e => 0xec_ec_e6,
        0xe0_31_31 => 0xff_5a_36,
        0x19_71_c2 => 0x9f_b4_ff,
        other => other,
    };
    format!("#{c:06x}")
}

/// Board units of ink per millisecond, as quick as a hand that knows what it is writing.
const PEN_SPEED: f32 = 1.4;
/// Room around the ink, and the least the canvas shows, so a small board is not blown up. A narrow
/// canvas shows less, so a phone still reads the writing.
const MARGIN: f64 = 80.0;
const MIN_VIEW: (f64, f64) = (900.0, 560.0);

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
    view: [f64; 4],
}

impl Board {
    fn new() -> Self {
        let root = q("#board");
        let (under, ink) = (svg_el("g", &[]), svg_el("g", &[]));
        let _ = root.append_child(&under);
        let _ = root.append_child(&ink);
        // No Chinese data here: the samples are English, and Latin strokes are built in.
        let glyphs = Glyphs::load(HanziSource::Directory(Default::default())).expect("built-in strokes");
        Self { svg: root, under, ink, glyphs, layout: StableLayout::default(), graph: Graph::default(), shown: HashSet::new(), view: [0.0, 0.0, MIN_VIEW.0, MIN_VIEW.1] }
    }

    fn clear(&mut self) {
        self.under.set_text_content(None);
        self.ink.set_text_content(None);
        self.layout = StableLayout::default();
        self.graph = Graph::default();
        self.shown.clear();
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
        for piece in &pieces {
            keys.insert(piece.key.clone());
            let new = !self.shown.contains(&piece.key);
            // The same seed in the same order gives the same wobble as in the app.
            let mut r = Rng::new(&piece.seed);
            for line in &piece.lines {
                let inked = line.inked(&mut r);
                let node = stroke_node(line.style);
                let layer = if line.style.under { &self.under } else { &self.ink };
                let _ = layer.append_child(&node);
                if new {
                    fresh.push(Drawing { node, inked });
                } else {
                    let _ = node.set_attribute("d", &path_of(&outline(&inked, 1.0)));
                }
            }
        }
        self.shown = keys;
        (fresh, laid.bounds)
    }

    fn set_view(&mut self, v: [f64; 4]) {
        self.view = v;
        let _ = self.svg.set_attribute("viewBox", &format!("{:.1} {:.1} {:.1} {:.1}", v[0], v[1], v[2], v[3]));
    }
}

/// The part of the board to show: all its ink, centred, never closer than a set size, in the canvas's shape.
fn frame_for(b: &Bounds) -> [f64; 4] {
    let (bw, bh, cx, cy) = if b.x1 > b.x0 { (b.x1 - b.x0, b.y1 - b.y0, (b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0) } else { (0.0, 0.0, 0.0, 0.0) };
    let r = q("#board").get_bounding_client_rect();
    let narrow = (r.width() / MIN_VIEW.0).clamp(0.45, 1.0);
    let margin = MARGIN * narrow;
    let (w, h) = ((bw + margin * 2.0).max(MIN_VIEW.0 * narrow), (bh + margin * 2.0).max(MIN_VIEW.1 * narrow));
    let aspect = if r.height() > 0.0 { r.width() / r.height() } else { 1.6 };
    let (w, h) = if w / h > aspect { (w, w / aspect) } else { (h * aspect, h) };
    [cx - w / 2.0, cy - h / 2.0, w, h]
}

fn stroke_node(style: Style) -> Element {
    svg_el("path", &[("fill", &chalk(style.rgb)), ("fill-opacity", &style.alpha.to_string())])
}

struct Ui {
    title: Element,
    state: Element,
    lines: Element,
    tabs: Vec<Element>,
    listen: Element,
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
}

/// Play one sample through. False if the reader started another one meanwhile.
async fn play(sample: &Sample, index: usize, board: &RefCell<Board>, ui: &Ui, control: &Control, me: u32) -> bool {
    let live = || control.run.get() == me;
    board.borrow_mut().clear();
    ui.lines.set_text_content(None);
    ui.title.set_text_content(Some(TITLES[index]));
    for (i, t) in ui.tabs.iter().enumerate() {
        let _ = t.class_list().toggle_with_force("on", i == index);
    }
    board.borrow_mut().set_view(frame_for(&Bounds { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 }));
    ui.set_state("listening");
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
        ease_view(board, frame_for(&bounds), &live).await;
        write(&fresh, &live).await;
        let _ = li.class_list().add_1("drawn");
        ui.set_state("ready");
        sleep(900.0).await;
    }
    live()
}

/// Move the view to where the new ink goes, quickly.
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
    let ui = Rc::new(Ui { title: q("#title"), state: q("#state"), lines: q("#lines"), tabs: tabs.clone(), listen: q("#listen") });
    let control = Rc::new(Control { wanted: Cell::new(None), paused: Cell::new(false), run: Cell::new(0) });
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
        });
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
