//! The window: the board, the transcript, and an input that accepts Chinese through the IME.

use boardcast_board::glyphs::{Glyphs, HanziSource, Pt};
use boardcast_board::board_layout::{target_at, Bounds as BoardBounds, Rect, StableLayout};
use boardcast_board::check::check;
use boardcast_board::compose::{compose, ink_boxes, InkBox, Owner, Style};
use boardcast_board::graph::{parse_ops, Graph, Kind, Op, Ref, Utterance};
use boardcast_agent::sound::Glossary;
use boardcast_board::follow;
use boardcast_board::sugiyama::first_diagram;
use boardcast_agent::settings::Settings;
use boardcast_agent::{propose, Backend, Guidance, LocalServer, Provider, Said, Update};
use boardcast_meetings::{default_dir, Info, Live, Store, ViewOnly};
use boardcast_board::pen::{marker, outline, Inked, Rng};
use crate::i18n::tr;
use crate::theme::{self, icon, pal};
use crate::pipeline::{Event, Pipeline, Script, Speech};
use crate::resources;
use boardcast_models::{Model, Outcome, Progress};
use boardcast_update::Release;
use crate::raster::Layer;
use anyhow::{Context as _, Result};
use boardcast_agent::summary::{self, Block, Line, Step};
use boardcast_agent::vision::VisionServer;
use boardcast_export::Kind as ExportKind;
use boardcast_import::pdf::NewPage;
use boardcast_board::spans::{span_at, Unit};
use boardcast_board::speakers::{by_of_edge, by_of_node, first_seen, initial, said_at};
use boardcast_board::steps::{clock, replay_to, Player};
use gpui_kit::base::input::{Textarea, TextareaState};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::separator::Separator;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::TitleBar;
use gpui_kit::component::{Disableable, Selectable, Sizable};
use gpui_kit::{
    canvas, div, point, prelude::*, px, rgb, size, App, Background, Bounds, Context, Entity, FillOptions, FocusHandle,
    Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, PathStyle, Pixels, Point, Render, RenderImage,
    ScrollEvent, Window, WindowBounds, WindowOptions,
};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the board stores, and what each is called on its button.
const MARKS: [(&str, &str); 5] = [("划掉", "Strike"), ("圈出", "Circle"), ("下划线", "Underline"), ("星标", "Star"), ("高亮", "Highlight")];
/// More meetings than this, and the list gets a search box.
const LIBRARY_SEARCH_FROM: usize = 5;
/// How much one press of a zoom key scales the board.
const ZOOM_STEP: f32 = 1.25;
/// Remote services filled in with one click: name, address and model.
const REMOTE_PRESETS: [(&str, &str, &str); 1] = [("DeepSeek", "https://api.deepseek.com", "deepseek-flash")];
/// The dock's height, along the board's lower edge.
const DOCK_H: f32 = 56.0;
/// Room kept clear around the board when it is fitted to the canvas.
const BOARD_MARGIN: f32 = 28.0;
/// The spacing of the grid drawn on the canvas, as on the website.
/// The spacing of the dots under the ink, fixed on the screen like the website's sheet.
const GRID: f32 = 24.0;
const AUTOSAVE_AFTER: Duration = Duration::from_millis(500);
const RETRY_SAVE_AFTER: Duration = Duration::from_secs(5);
#[derive(Clone, Copy, PartialEq)]
enum MarkUnit {
    Word,
    Sentence,
    Block,
}

/// A board with this many things on it is full enough that a new topic is offered a board of its own.
const BUSY_BOARD: usize = 8;
/// Replies an offer of a board for a new topic stays for, if nobody takes it.
const TOPIC_HINT_REPLIES: u8 = 3;
/// Lines after turning to another board during which no other one is offered.
const QUIET_AFTER_TURNING: u8 = 3;
const TOOLBAR_W: f32 = 290.0;
const TOOLBAR_W_WITH_UNITS: f32 = 520.0;
const TOOLBAR_W_SPEAKER: f32 = 110.0;
const TOOLBAR_H: f32 = 36.0;
/// Room for rewriting and removing, which imported text does not offer.
const TOOLBAR_W_EDIT: f32 = 72.0;
/// The bar while a label is being rewritten.
const TOOLBAR_W_EDITING: f32 = 300.0;
const PREPARING_PAGE_AFTER: Duration = Duration::from_secs(5);
/// How much of the progress bar is estimated, ahead of the part that counts real tokens.
const ESTIMATED_SHARE: f32 = 0.7;
/// The width of what floats at the side, how far things float from the edge, and the room kept for the
/// bars above and below the board.
const SIDE_W: f32 = 340.0;
const JUMP_SHOWN: Duration = Duration::from_millis(2500);
const JUMP_SCROLLING: Duration = Duration::from_millis(300);
/// A heard line is typed in a few characters at a time, as the web demo does.
const TYPE_STEP: Duration = Duration::from_millis(28);
const TYPE_CHARS: usize = 3;

/// The line being typed into the transcript, and whether the list follows it to the bottom.
struct Typing {
    line: usize,
    started: Instant,
    /// Typed lines appear whole; heard ones are typed in.
    animate: bool,
    /// The reader was at the newest line, so the list keeps it in view. Scrolled back, they are left there.
    follow: bool,
}

impl Typing {
    fn chars(&self) -> usize {
        TYPE_CHARS * (self.started.elapsed().as_millis() / TYPE_STEP.as_millis()).saturating_add(1) as usize
    }
}
/// How far everything else fades while one speaker is traced.
const TRACE_VEIL: f32 = 0.7;
const TRACE_MARK: f32 = 0.5;
/// A press that moves less than this is a click, not a pan.
const CLICK_SLOP: f32 = 4.0;
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 4.0;
/// A new sharpness level is rendered once zooming has paused this long.
const RERASTER_AFTER: Duration = Duration::from_millis(120);
/// How long the view takes to move to where new content needs it, easing out.
const GLIDE: Duration = Duration::from_millis(450);
/// How long ink takes to make room for a line written in above it.
const SLIDE: Duration = Duration::from_millis(220);
/// How long after the user last moved the view new ink waits before taking the view back to it.
const READ_HOLD: Duration = Duration::from_secs(5);
/// Writing speed in board units, as on the website's demo: a hand that knows what it is writing.
const PEN_PX_PER_SECOND: f32 = 1400.0;
/// A backlog is written faster, so what is waiting never takes longer than this to appear.
const PEN_BACKLOG: f32 = 3.5;
/// A mark made in one stroke (a star, a ring, an underline) takes at least this long, so it is seen being drawn.
const MARK_SECONDS: f32 = 0.35;

struct Queued {
    inked: Inked,
    style: Style,
    key: String,
    /// The batch this stroke belongs to, for end-to-end timing.
    batch: u64,
    /// Whether the stroke is a whole piece on its own, a mark rather than part of a word.
    alone: bool,
}

struct Stroke {
    key: String,
    shape: Vec<Pt>,
    style: Style,
    area: Area,
}

/// A rectangle on the board: left, top, right, bottom.
type Area = [f32; 4];

fn area_of(shape: &[Pt]) -> Area {
    shape.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, p| [a[0].min(p[0]), a[1].min(p[1]), a[2].max(p[0]), a[3].max(p[1])])
}

fn meets(a: Area, b: Area) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn inside(a: Area, b: Area) -> bool {
    a[0] >= b[0] && a[1] >= b[1] && a[2] <= b[2] && a[3] <= b[3]
}

/// Highlights under the ink, and the ink, at one sharpness.
struct Tier {
    under: Layer,
    layer: Layer,
    level: f32,
}

impl Tier {
    fn new(level: f32) -> Self {
        Self { under: Layer::with_scale(level), layer: Layer::with_scale(level), level }
    }

    fn fill(&mut self, stroke: &Stroke) {
        let [_, r, g, b] = pal().ink(stroke.style.rgb).to_be_bytes();
        let target = if stroke.style.under { &mut self.under } else { &mut self.layer };
        target.fill(&stroke.shape, [r, g, b], (stroke.style.alpha * 255.0) as u8);
    }

    fn release(self) -> Vec<Arc<RenderImage>> {
        let mut images = self.under.release();
        images.extend(self.layer.release());
        images
    }

    fn uploads(&self) -> usize {
        self.under.uploads + self.layer.uploads
    }
}

/// Board coordinates at the canvas origin, and screen pixels per board unit.
#[derive(Clone, Copy, PartialEq, Debug)]
struct View {
    x: f32,
    y: f32,
    k: f32,
}

struct Rebuilt {
    tier: Tier,
    /// The part of the board a detail tier covers; None for the whole-board base.
    cover: Option<Area>,
    /// How many finished strokes this rendering covers; later ones are added on arrival.
    covers: usize,
    generation: u64,
    ms: u128,
}

/// Everything the canvas needs while painting. Finished strokes live in textures.
struct Ink {
    glyphs: Glyphs,
    board: StableLayout,
    /// The whole board at the sharpness of the fit view. Cheap, and always ready when panning.
    base: Tier,
    /// Just around the viewport at the current zoom. Memory follows the window, not the board.
    detail: Option<(Tier, Area)>,
    retired: Vec<Arc<RenderImage>>,
    queue: VecDeque<Queued>,
    live: f32,
    finished: Vec<Arc<Stroke>>,
    /// Ink pushed along by something written in above it: each stroke where it now belongs, and how far
    /// back it started, eased into place before it joins the textures.
    sliding: Vec<(Arc<Stroke>, [f32; 2])>,
    slide_started: Instant,
    drawn: HashSet<String>,
    last: Option<Instant>,
    timings: Vec<Timing>,
    graph: Option<Graph>,
    next_tidy_batch: u64,
    /// What a click can land on, in board coordinates.
    targets: Vec<Rect>,
    /// Where each thing's ink lies, in board coordinates.
    ink_boxes: Vec<InkBox>,
    /// Whose contributions stand out, and the color that marks them.
    trace: Option<(String, u32)>,
    /// The thing whose mark toolbar is open.
    selected: Option<String>,
    /// Where the selection was clicked, in board coordinates, to find the word there.
    selected_at: Option<(f64, f64)>,
    /// How much of imported text a mark covers.
    mark_unit: MarkUnit,
    /// The imported page under the board, with its size on the board.
    page: Option<(Arc<RenderImage>, f32, f32)>,
    /// What the layout check wants the model to hear next time.
    hints: Option<String>,
    /// The content's extent, which the automatic view fits.
    extent: BoardBounds,
    /// None follows the whole board. Some keeps a view the content still fits in, or one the user chose.
    view: Option<View>,
    /// The user zoomed or panned: new content must not move the view until they return to latest.
    reading: bool,
    /// The view moves on its own from here, starting then, rather than jumping.
    glide: Option<(View, Instant)>,
    /// A label is being rewritten in the selection's bar.
    editing: bool,
    canvas: Bounds<Pixels>,
    /// What floats over each side of the canvas, as left, top, right and bottom. The whole board fits
    /// between them, while the board still runs on underneath.
    insets: [f32; 4],
    dpr: f32,
    zoomed_at: Instant,
    /// When the user last moved or zoomed the view by hand. Reading holds the view only for a while after.
    touched: Instant,
    /// Bumped when finished strokes are removed, which invalidates a rendering in progress.
    generation: u64,
    rebuilding: Option<Receiver<Rebuilt>>,
    drag: Option<(Point<Pixels>, View)>,
    /// Dragging draws a circle around things rather than moving the board.
    circling: bool,
    /// Circles drawn so far, and the one being drawn, in board coordinates.
    loops: Vec<Vec<[f32; 2]>>,
    drawing: Option<Vec<[f32; 2]>>,
    /// What the circles took in, first circled first; the next line carries it to the model.
    pointed: Vec<String>,
    zoom_test: Option<ZoomTest>,
    instant: bool,
}

/// A scripted zoom sweep that reports what zooming costs.
struct ZoomTest {
    started: Option<Instant>,
    frames: Vec<f32>,
    last: Option<Instant>,
    base: f32,
    rebuilds: Vec<u128>,
    peak_rss: u64,
}

/// Quantized so a smooth zoom re-renders rarely: half-octave steps, rounded up to stay sharp.
fn raster_level(k: f32, dpr: f32) -> f32 {
    let steps = ((k * dpr).max(0.25).log2() * 2.0).ceil();
    2.0_f32.powf(steps / 2.0).clamp(0.25, 8.0)
}

struct Timing {
    id: u64,
    text: String,
    asr_ms: Option<u128>,
    llm_ms: u128,
    batch: usize,
    ended: Instant,
    arrived: Instant,
    first_ink: Option<Instant>,
    strokes: usize,
}

/// Whether a closed shape holds a point, by counting crossings.
fn encloses(shape: &[[f32; 2]], p: [f32; 2]) -> bool {
    let mut inside = false;
    let mut j = shape.len() - 1;
    for i in 0..shape.len() {
        let (a, b) = (shape[i], shape[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn fill_builder() -> PathBuilder {
    PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::non_zero()))
}

fn color(style: Style) -> Hsla {
    let mut c: Hsla = rgb(pal().ink(style.rgb)).into();
    c.a = style.alpha;
    c
}

impl Ink {
    fn retire_detail(&mut self) {
        if let Some((tier, _)) = self.detail.take() {
            self.retired.extend(tier.release());
        }
    }

    /// Take a new board: keep ink whose piece survives, redraw the rest stroke by stroke.
    fn update(&mut self, graph: &Graph, batch: u64) -> usize {
        self.graph = Some(graph.clone());
        let mut layout = self.board.layout(graph, &mut first_diagram);
        let (mut pieces, mut unclear) = compose(graph, &layout, &mut self.glyphs);
        let mut report = check(graph, &layout, &unclear);
        // A real defect outweighs keeping things in place, unless the user is reading somewhere.
        if !report.hard.is_empty() && !self.reading {
            println!("{{\"auto_tidy\":{}}}", report.to_json());
            self.board = StableLayout::default();
            layout = self.board.layout(graph, &mut first_diagram);
            (pieces, unclear) = compose(graph, &layout, &mut self.glyphs);
            report = check(graph, &layout, &unclear);
        }
        self.hints = report.hint();
        self.targets = layout.targets();
        self.ink_boxes = ink_boxes(&pieces);
        if let Some(id) = &self.selected {
            if !graph.nodes.iter().any(|n| &n.id == id) {
                self.selected = None;
            }
        }
        self.follow(&layout.bounds);
        self.extent = layout.bounds;
        self.settle_slides();
        let wanted: HashSet<&str> = pieces.iter().map(|p| p.key.as_str()).collect();
        // Ink that only moved, because something went in above it, keeps its strokes: they are taken by
        // what the piece is (its seed), and slide to where it now sits instead of being written again.
        let seed_of = |key: &str| key.rsplit_once('@').map_or(key, |(seed, _)| seed).to_string();
        let mut gone: HashMap<String, Vec<Arc<Stroke>>> = HashMap::new();
        for stroke in self.finished.iter().filter(|s| !wanted.contains(s.key.as_str())) {
            gone.entry(seed_of(&stroke.key)).or_default().push(stroke.clone());
        }
        // A piece already written may change its look in place, as words fade when they are crossed out.
        let styles: HashMap<&str, Style> = pieces.iter().filter_map(|p| p.lines.first().map(|l| (p.key.as_str(), l.style))).collect();
        let mut restyled = false;
        for stroke in &mut self.finished {
            if let Some(style) = styles.get(stroke.key.as_str()).filter(|s| **s != stroke.style) {
                *stroke = Arc::new(Stroke { key: stroke.key.clone(), shape: stroke.shape.clone(), style: *style, area: stroke.area });
                restyled = true;
            }
        }
        for queued in &mut self.queue {
            if let Some(style) = styles.get(queued.key.as_str()) {
                queued.style = *style;
            }
        }
        // Ink cannot be partly erased from a texture, so a removal, a move or a new look repaints what remains.
        if restyled || self.drawn.iter().any(|k| !wanted.contains(k.as_str())) {
            self.finished.retain(|s| wanted.contains(s.key.as_str()));
            self.base.under.clear();
            self.base.layer.clear();
            for stroke in &self.finished {
                self.base.fill(stroke);
            }
            self.retire_detail();
            self.drawn.retain(|k| wanted.contains(k.as_str()));
            self.generation += 1;
        }
        let in_progress = self.queue.front().map(|q| q.key.clone());
        self.queue.retain(|q| wanted.contains(q.key.as_str()));
        if self.queue.front().map(|q| &q.key) != in_progress.as_ref() {
            self.live = 0.0;
        }
        let queued: HashSet<String> = self.queue.iter().map(|q| q.key.clone()).collect();
        let mut added = 0;
        let mut moved = vec![];
        for piece in pieces {
            if self.drawn.contains(&piece.key) || queued.contains(&piece.key) {
                continue;
            }
            let mut r = Rng::new(&piece.seed);
            if let Some(before) = gone.remove(&piece.seed).filter(|b| b.len() == piece.lines.len()) {
                for (line, old) in piece.lines.iter().zip(before) {
                    let shape = outline(&line.inked(&mut r), 1.0);
                    let from = match (old.shape.first(), shape.first()) {
                        (Some(a), Some(b)) => [a[0] - b[0], a[1] - b[1]],
                        _ => [0.0, 0.0],
                    };
                    moved.push((Arc::new(Stroke { key: piece.key.clone(), area: area_of(&shape), shape, style: line.style }), from));
                }
                self.drawn.insert(piece.key);
                continue;
            }
            let alone = piece.lines.len() == 1;
            for line in &piece.lines {
                self.queue.push_back(Queued {
                    inked: line.inked(&mut r),
                    style: line.style,
                    key: piece.key.clone(),
                    batch,
                    alone,
                });
                added += 1;
            }
            self.drawn.insert(piece.key);
        }
        if !moved.is_empty() {
            self.sliding = moved;
            self.slide_started = Instant::now();
        }
        added
    }

    /// Forget every position and lay the board out again in one go.
    fn tidy(&mut self) {
        let Some(graph) = self.graph.clone() else { return };
        self.board = StableLayout::default();
        self.next_tidy_batch += 1;
        let batch = u64::MAX - self.next_tidy_batch;
        self.show_latest();
        self.update(&graph, batch);
    }

    /// What to bring back out from under the veil when a speaker is traced: patches of ink, and what
    /// to mark with their color, both in board coordinates.
    fn traced_areas(&self, who: &str) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
        let Some(graph) = &self.graph else { return (vec![], vec![]) };
        let nodes: HashSet<&str> = graph.nodes.iter().filter(|n| by_of_node(n) == Some(who)).map(|n| n.id.as_str()).collect();
        let edges: HashSet<(&str, &str)> = graph.edges.iter().filter(|e| by_of_edge(e) == Some(who)).map(|e| (e.from.as_str(), e.to.as_str())).collect();
        let area = |b: &InkBox| [b.x, b.y, b.w, b.h];
        let raised = |b: &&InkBox| match &b.owner {
            Owner::Node(id) => nodes.contains(id.as_str()),
            Owner::Edge(from, to) => edges.contains(&(from.as_str(), to.as_str())),
        };
        let shown = self.ink_boxes.iter().filter(raised);
        let marked = self.targets.iter().filter(|t| nodes.contains(t.id.as_str())).map(|t| [t.x as f32, t.y as f32, t.w as f32, t.h as f32]);
        let labels = shown.clone().filter(|b| b.label).map(area);
        (shown.map(area).collect(), marked.chain(labels).collect())
    }

    /// A click rather than a drag: select what is under the pointer, or clear the selection.
    fn click(&mut self, at: Point<Pixels>) {
        let v = self.shown();
        let x = v.x + (f32::from(at.x) - f32::from(self.canvas.origin.x)) / v.k;
        let y = v.y + (f32::from(at.y) - f32::from(self.canvas.origin.y)) / v.k;
        self.selected = target_at(&self.targets, f64::from(x), f64::from(y)).map(|t| t.id.clone());
        self.selected_at = self.selected.as_ref().map(|_| (f64::from(x), f64::from(y)));
        self.mark_unit = MarkUnit::Block;
    }

    /// A point on the canvas, in board coordinates.
    fn on_board(&self, at: Point<Pixels>) -> [f32; 2] {
        let v = self.shown();
        [v.x + (f32::from(at.x) - f32::from(self.canvas.origin.x)) / v.k, v.y + (f32::from(at.y) - f32::from(self.canvas.origin.y)) / v.k]
    }

    /// Close the circle being drawn, and take in whatever has its middle inside it.
    fn close_loop(&mut self) {
        let Some(shape) = self.drawing.take().filter(|s| s.len() >= 3) else { return };
        for t in &self.targets {
            let middle = [(t.x + t.w / 2.0) as f32, (t.y + t.h / 2.0) as f32];
            if encloses(&shape, middle) && !self.pointed.contains(&t.id) {
                self.pointed.push(t.id.clone());
            }
        }
        let over = self.pointed.len().saturating_sub(boardcast_board::graph::MAX_POINTED);
        self.pointed.drain(..over);
        self.loops.push(shape);
    }

    fn clear_circles(&mut self) {
        self.circling = false;
        self.loops.clear();
        self.drawing = None;
        self.pointed.clear();
    }

    /// Where the toolbar goes, in canvas pixels: under the selection, kept inside the canvas.
    fn toolbar_at(&self) -> Option<(f32, f32)> {
        let id = self.selected.as_ref()?;
        let t = self.targets.iter().find(|t| &t.id == id)?;
        let v = self.shown();
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        let x = (t.x as f32 - v.x) * v.k;
        let below = (t.y as f32 + t.h as f32 - v.y) * v.k + 8.0;
        let above = (t.y as f32 - v.y) * v.k - TOOLBAR_H - 8.0;
        let y = if below + TOOLBAR_H > h { above } else { below };
        // Imported text also offers how much to mark, which makes the bar wider.
        let has_text = self.graph.as_ref().and_then(|g| g.nodes.iter().find(|n| &n.id == id)).is_some_and(|n| !n.rows().is_empty());
        let raised = self.graph.as_ref().and_then(|g| g.nodes.iter().find(|n| &n.id == id)).is_some_and(|n| by_of_node(n).is_some());
        let fixed = self.graph.as_ref().and_then(|g| g.nodes.iter().find(|n| &n.id == id)).is_some_and(|n| n.kind == Kind::Region);
        let width = match self.editing {
            true => TOOLBAR_W_EDITING,
            false => (if has_text { TOOLBAR_W_WITH_UNITS } else { TOOLBAR_W }) + if raised { TOOLBAR_W_SPEAKER } else { 0.0 } + if fixed { 0.0 } else { TOOLBAR_W_EDIT },
        };
        Some((x.clamp(8.0, (w - width - 8.0).max(8.0)), y.clamp(8.0, (h - TOOLBAR_H - 8.0).max(8.0))))
    }

    /// Draw what is on the board again, in the palette now in use.
    fn redraw(&mut self) {
        let level = self.base.level;
        self.retired.extend(std::mem::replace(&mut self.base, Tier::new(level)).release());
        self.retire_detail();
        for stroke in self.finished.clone() {
            self.base.fill(&stroke);
        }
    }

    /// Light and dark changed: repaint the ink in the new palette, and drop a sharper rendering still being
    /// made in the old one.
    fn recolor(&mut self) {
        self.generation += 1;
        self.rebuilding = None;
        self.redraw();
    }

    /// Show another meeting: nothing of the old board remains, and the new one appears at once
    /// rather than being rewritten stroke by stroke.
    fn open(&mut self, graph: &Graph, memory: &serde_json::Value) {
        let level = self.base.level;
        self.retired.extend(std::mem::replace(&mut self.base, Tier::new(level)).release());
        self.retire_detail();
        self.queue.clear();
        self.finished.clear();
        self.sliding.clear();
        self.drawn.clear();
        self.timings.clear();
        self.targets.clear();
        self.live = 0.0;
        self.selected = None;
        self.hints = None;
        self.view = None;
        self.reading = false;
        self.glide = None;
        self.drag = None;
        self.extent = BoardBounds::default();
        self.generation += 1;
        self.rebuilding = None;
        self.board = StableLayout::default();
        self.board.restore(memory);
        self.instant = true;
        self.update(graph, 0);
    }

    /// Fit everything: never above actual size, so a small board is not blown up.
    fn fit(&self) -> View {
        let [left, top, right, bottom] = self.insets;
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        let (free_w, free_h) = ((w - left - right).max(1.0), (h - top - bottom).max(1.0));
        let e = &self.extent;
        let bw = (e.x1 - e.x0) as f32 + 120.0;
        let bh = (e.y1 - e.y0) as f32 + 120.0;
        let k = (free_w / bw).min(free_h / bh).min(1.0);
        View { k, x: (e.x0 + e.x1) as f32 / 2.0 - (left + free_w / 2.0) / k, y: (e.y0 + e.y1) as f32 / 2.0 - (top + free_h / 2.0) / k }
    }

    /// The controls float somewhere new: unless the user chose the view, fit the board between them.
    fn set_insets(&mut self, insets: [f32; 4]) {
        if self.insets != insets {
            if !self.reading {
                self.glide_from_here();
                self.view = None;
            }
            self.insets = insets;
        }
    }

    /// Where the view is going: the one kept, or the fit.
    fn target(&self) -> View {
        self.view.unwrap_or_else(|| self.fit())
    }

    /// The view on screen now: the target, or on the way to it. The scale changes by the same ratio each step
    /// and the centre moves in a straight line, so the board neither swings aside nor overshoots.
    fn shown(&self) -> View {
        let to = self.target();
        let Some((from, started)) = self.glide else { return to };
        let p = started.elapsed().as_secs_f32() / GLIDE.as_secs_f32();
        if p >= 1.0 || from.k <= 0.0 || to.k <= 0.0 {
            return to;
        }
        let e = 1.0 - (1.0 - p).powi(3);
        let (hw, hh) = (f32::from(self.canvas.size.width) / 2.0, f32::from(self.canvas.size.height) / 2.0);
        let centre = |v: View| (v.x + hw / v.k, v.y + hh / v.k);
        let (a, b) = (centre(from), centre(to));
        let k = from.k * (to.k / from.k).powf(e);
        let (cx, cy) = (a.0 + (b.0 - a.0) * e, a.1 + (b.1 - a.1) * e);
        View { k, x: cx - hw / k, y: cy - hh / k }
    }

    fn gliding(&self) -> bool {
        self.glide.is_some_and(|(_, started)| started.elapsed() < GLIDE)
    }

    /// The view is about to change on its own: move there from what is on screen instead of jumping.
    /// Opening a meeting or seeking a replay shows the result at once.
    fn glide_from_here(&mut self) {
        self.glide = (!self.instant).then(|| (self.shown(), Instant::now()));
    }

    /// New content keeps the view while it fits, so the board does not rescale with every line.
    fn follow(&mut self, next: &BoardBounds) {
        // Measured against where the view is going, so a move already under way is not cut short.
        let cur = self.target();
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        let [left, top, right, bottom] = self.insets;
        let inside = self.extent.x1 > self.extent.x0
            && next.x0 as f32 >= cur.x + left / cur.k
            && next.y0 as f32 >= cur.y + top / cur.k
            && next.x1 as f32 <= cur.x + (w - right) / cur.k
            && next.y1 as f32 <= cur.y + (h - bottom) / cur.k;
        // Someone looking around keeps their view while what is new shows in it, or while they are still
        // moving about; after that, new ink out of sight brings the view back to follow the talk.
        if self.reading {
            if inside || self.touched.elapsed() < READ_HOLD {
                return;
            }
            self.reading = false;
        }
        let view = inside.then_some(cur);
        if view.map(|v| v.k) != self.view.map(|v| v.k) {
            self.zoomed_at = Instant::now();
        }
        if view.is_none() {
            self.glide_from_here();
        }
        self.view = view;
    }

    /// Zoom around a point on the canvas, as the current app does.
    fn zoom_at(&mut self, at: Point<Pixels>, factor: f32) {
        let v = self.shown();
        let (px_, py) = (f32::from(at.x) - f32::from(self.canvas.origin.x), f32::from(at.y) - f32::from(self.canvas.origin.y));
        let k = (v.k * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (mx, my) = (v.x + px_ / v.k, v.y + py / v.k);
        self.view = Some(View { k, x: mx - px_ / k, y: my - py / k });
        self.reading = true;
        self.glide = None;
        self.zoomed_at = Instant::now();
        self.touched = Instant::now();
    }

    fn show_latest(&mut self) {
        self.glide_from_here();
        self.reading = false;
        self.view = None;
        self.zoomed_at = Instant::now();
    }

    /// The board area visible now.
    fn visible(&self) -> Area {
        let v = self.shown();
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        [v.x, v.y, v.x + w / v.k, v.y + h / v.k]
    }

    /// Render finished ink at the sharpness this view needs, off the UI thread. Until it arrives
    /// the existing textures stretch, so zooming and panning never wait for the rasterizer.
    fn maintain_sharpness(&mut self) {
        if let Some(rx) = &self.rebuilding {
            let Ok(done) = rx.try_recv() else { return };
            self.rebuilding = None;
            if done.generation == self.generation {
                let mut tier = done.tier;
                for stroke in &self.finished[done.covers..] {
                    if done.cover.is_none_or(|c| meets(stroke.area, c)) {
                        tier.fill(stroke);
                    }
                }
                if let Some(cover) = done.cover {
                    self.retire_detail();
                    self.detail = Some((tier, cover));
                } else {
                    let old = std::mem::replace(&mut self.base, tier);
                    self.retired.extend(old.release());
                }
                if let Some(test) = &mut self.zoom_test {
                    test.rebuilds.push(done.ms);
                }
            }
        }
        // Not while the view is still moving: the next frame would want a different sharpness.
        if self.zoomed_at.elapsed() < RERASTER_AFTER || self.gliding() {
            return;
        }
        let k = self.shown().k;
        let base_wanted = raster_level(k.min(self.fit().k), self.dpr);
        let wanted = raster_level(k, self.dpr);
        let job = if base_wanted != self.base.level {
            Some((base_wanted, None))
        } else if wanted <= self.base.level {
            self.retire_detail();
            None
        } else {
            let view = self.visible();
            let current = self.detail.as_ref().is_some_and(|(t, cover)| t.level == wanted && inside(view, *cover));
            // One screen of margin on every side, so a short pan is already sharp.
            let (w, h) = (view[2] - view[0], view[3] - view[1]);
            (!current).then_some((wanted, Some([view[0] - w, view[1] - h, view[2] + w, view[3] + h])))
        };
        let Some((level, cover)) = job else { return };
        let strokes = self.finished.clone();
        let generation = self.generation;
        let (tx, rx) = channel();
        self.rebuilding = Some(rx);
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut tier = Tier::new(level);
            for s in &strokes {
                if cover.is_none_or(|c: Area| meets(s.area, c)) {
                    tier.fill(s);
                }
            }
            let _ = tx.send(Rebuilt { tier, cover, covers: strokes.len(), generation, ms: started.elapsed().as_millis() });
        });
    }

    /// During a zoom test, says what each slow frame was doing, so a stall can be attributed.
    fn note_frame(&mut self, paint: Duration, uploads_before: usize) {
        let uploads = self.base.uploads() + self.detail.as_ref().map_or(0, |(t, _)| t.uploads()) - uploads_before;
        let k = self.shown().k;
        let detail = self.detail.is_some();
        let rebuilding = self.rebuilding.is_some();
        let Some(test) = &mut self.zoom_test else { return };
        let gap = test.frames.last().copied().unwrap_or(0.0);
        if gap > 50.0 || paint.as_millis() > 20 {
            eprintln!("slow frame: gap {gap:.0} ms, paint {} ms, uploads {uploads}, k {k:.2}, detail {detail}, rebuilding {rebuilding}", paint.as_millis());
        }
    }

    /// Advance the scripted sweep by one frame; returns true while it is running. It pauses at
    /// each extreme, because the sharper textures are only built once the view stops moving.
    fn sweep(&mut self) -> bool {
        if !self.queue.is_empty() {
            return false;
        }
        let k_fit = self.fit().k;
        let Some(test) = &mut self.zoom_test else { return false };
        let now = Instant::now();
        let started = *test.started.get_or_insert(now);
        if test.base == 0.0 {
            test.base = k_fit;
        }
        if let Some(last) = test.last.replace(now) {
            test.frames.push((now - last).as_secs_f32() * 1000.0);
        }
        let t = (now - started).as_secs_f32();
        let base = test.base;
        let ease = |from: f32, to: f32, p: f32| from * (to / from).powf(p.clamp(0.0, 1.0));
        // In to 4x, hold, pan across the board, hold, out to the minimum, hold, back to the fit.
        let (k, pan, moving) = match t {
            t if t < 2.0 => (ease(base, MAX_ZOOM, t / 2.0), 0.0, true),
            t if t < 3.0 => (MAX_ZOOM, 0.0, false),
            t if t < 6.0 => (MAX_ZOOM, (t - 3.0) * 800.0, false),
            t if t < 7.0 => (MAX_ZOOM, 2400.0, false),
            t if t < 9.0 => (ease(MAX_ZOOM, MIN_ZOOM, (t - 7.0) / 2.0), 0.0, true),
            t if t < 10.0 => (MIN_ZOOM, 0.0, false),
            t if t < 12.0 => (ease(MIN_ZOOM, base, (t - 10.0) / 2.0), 0.0, true),
            _ => (base, 0.0, false),
        };
        test.peak_rss = test.peak_rss.max(rss_mb());
        if t > 12.5 {
            let mut f = std::mem::take(&mut test.frames);
            f.sort_by(f32::total_cmp);
            let q = |p: f32| f[((f.len() - 1) as f32 * p) as usize];
            println!(
                "{{\"zoom_test\":true,\"strokes\":{},\"frames\":{},\"frame_p50\":{:.2},\"frame_p95\":{:.2},\"frame_p99\":{:.2},\"frame_max\":{:.1},\"rebuilds_ms\":{:?},\"peak_rss_mb\":{}}}",
                self.finished.len(), f.len(), q(0.5), q(0.95), q(0.99), f[f.len() - 1], test.rebuilds, test.peak_rss,
            );
            std::process::exit(0);
        }
        let e = &self.extent;
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        self.view = Some(View { k, x: (e.x0 + e.x1) as f32 / 2.0 - w / (2.0 * k) + pan / k, y: (e.y0 + e.y1) as f32 / 2.0 - h / (2.0 * k) });
        self.reading = true;
        if moving {
            self.zoomed_at = now;
        }
        true
    }

    fn advance(&mut self) {
        if !self.sliding.is_empty() && (self.instant || self.slide_left().is_none()) {
            self.settle_slides();
        }
        if self.queue.is_empty() {
            self.instant = false;
        }
        let now = Instant::now();
        // The view gets there first, then the pen writes, so new ink is never started off screen.
        if self.gliding() && !self.instant {
            self.last = Some(now);
            return;
        }
        let dt = self.last.replace(now).map_or(0.016, |l| (now - l).as_secs_f32()).min(0.1);
        // Time to spend this frame, and the pen's speed for what is waiting.
        let mut budget = if self.instant { f32::MAX } else { dt };
        let waiting: f32 = self.queue.iter().map(|q| q.inked.length).sum::<f32>() - self.queue.front().map_or(0.0, |q| q.inked.length * self.live);
        let speed = PEN_PX_PER_SECOND.max(waiting / PEN_BACKLOG);
        while budget > 0.0 {
            let Some(front) = self.queue.front() else { break };
            let (id, length) = (front.batch, front.inked.length);
            if let Some(t) = self.timings.iter_mut().find(|t| t.id == id && t.first_ink.is_none()) {
                t.first_ink = Some(now);
            }
            let rate = if front.alone { speed.min(length / MARK_SECONDS).max(1.0) } else { speed };
            let left = length * (1.0 - self.live) / rate;
            if left > budget {
                self.live += budget * rate / length.max(0.01);
                return;
            }
            budget -= left;
            self.live = 0.0;
            let done = self.queue.pop_front().expect("front exists");
            let shape = outline(&done.inked, 1.0);
            self.bake(Arc::new(Stroke { key: done.key, area: area_of(&shape), shape, style: done.style }));
            if !self.queue.iter().any(|q| q.batch == done.batch) {
                self.report(done.batch, now);
            }
        }
    }

    /// Put a finished stroke on the textures.
    fn bake(&mut self, stroke: Arc<Stroke>) {
        self.base.fill(&stroke);
        if let Some((tier, cover)) = &mut self.detail {
            if meets(stroke.area, *cover) {
                tier.fill(&stroke);
            }
        }
        self.finished.push(stroke);
    }

    /// Ink that has slid into place joins the textures.
    fn settle_slides(&mut self) {
        for (stroke, _) in std::mem::take(&mut self.sliding) {
            self.bake(stroke);
        }
    }

    /// How far back sliding ink still is, eased out; None once it has arrived.
    fn slide_left(&self) -> Option<f32> {
        let k = self.slide_started.elapsed().as_secs_f32() / SLIDE.as_secs_f32();
        (!self.sliding.is_empty() && k < 1.0).then(|| (1.0 - k).powi(3))
    }

    fn report(&mut self, id: u64, now: Instant) {
        let Some(i) = self.timings.iter().position(|t| t.id == id) else { return };
        let t = self.timings.remove(i);
        let first = t.first_ink.unwrap_or(now);
        println!(
            "{{\"line\":{},\"batch\":{},\"asr_ms\":{},\"llm_ms\":{},\"to_board_ms\":{},\"to_first_ink_ms\":{},\"to_done_ms\":{},\"ui_delay_ms\":{},\"strokes\":{}}}",
            serde_json::Value::from(t.text),
            t.batch,
            t.asr_ms.map_or("null".to_string(), |v| v.to_string()),
            t.llm_ms,
            (t.arrived - t.ended).as_millis(),
            (first - t.ended).as_millis(),
            (now - t.ended).as_millis(),
            (first - t.arrived).as_millis(),
            t.strokes,
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SaveState {
    Idle,
    Dirty,
    Saving,
    Saved,
    Failed(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Transcript,
    Library,
    Summary,
    Export,
    Import,
    Speakers,
    Settings,
}

impl Panel {
    /// Whether the panel is used alongside the board, so it lives in the side; the rest cover the board.
    fn beside_board(self) -> bool {
        matches!(self, Panel::Transcript | Panel::Summary | Panel::Speakers)
    }
}

/// The meeting search worker, and what it has said.
struct MeetingSearch {
    jobs: std::sync::mpsc::Sender<boardcast_search::service::Job>,
    events: Receiver<boardcast_search::service::Event>,
    /// Meetings looked at in the current round of indexing, of how many.
    indexing: Option<(usize, usize)>,
    /// The query asked last, its ticket, and when it was typed, so a query waits for typing to pause.
    asked: String,
    typed_at: Option<Instant>,
    ticket: u64,
    /// A question has been asked and its answer has not come back yet.
    asking: bool,
    hits: Vec<boardcast_search::service::Found>,
    /// Meetings saved since they were last indexed, with when. Each is indexed again a while after its last
    /// save, and never while listening.
    saved: HashMap<String, Instant>,
    error: Option<String>,
}

/// How long typing must pause before a query is asked, and a save before the meeting is indexed again.
const QUERY_AFTER: Duration = Duration::from_millis(350);
const REINDEX_AFTER: Duration = Duration::from_secs(10);

/// What the board model is doing. Kept as a state and put in words when shown, so it follows the
/// interface language; once the model is ready there is nothing to say.
#[derive(Clone, Copy, PartialEq)]
enum ModelStatus {
    Quiet,
    Starting,
    Loading,
    NoModel,
}

impl ModelStatus {
    fn text(self) -> Option<&'static str> {
        match self {
            ModelStatus::Quiet => None,
            ModelStatus::Starting => Some(tr("Starting…", "启动中…")),
            ModelStatus::Loading => Some(tr("Loading the board model…", "正在加载板书模型…")),
            ModelStatus::NoModel => Some(tr("Choose a board model to start", "请先选择板书模型")),
        }
    }
}

/// The parts of the settings page.
#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    Model,
    Speech,
    General,
    About,
}

/// What reads text in a picture on this computer: llama-server, a local model that can see, and its add-on.
#[derive(Clone)]
struct Vision {
    binaries: PathBuf,
    model: PathBuf,
    projector: PathBuf,
}

enum ImportSource {
    File(PathBuf),
    Arxiv(String),
    /// An image, with the local model and add-on that will read it.
    Image { path: PathBuf, binaries: PathBuf, model: PathBuf, projector: PathBuf },
}

/// A file read and ready. Nothing joins a meeting until a place for it is chosen.
struct ImportPreview {
    name: String,
    pages: Vec<NewPage>,
    title: Option<String>,
    /// What goes in the transcript.
    note: String,
    /// What was left out or is worth knowing.
    details: Vec<String>,
    /// What the file is good for in the meeting, shown before choosing where it goes.
    ready: String,
}

enum ImportUpdate {
    Progress(String),
    Finished(Result<ImportPreview, String>),
}

enum ImportState {
    Idle,
    Reading { progress: String, cancel: Arc<AtomicBool>, updates: Receiver<ImportUpdate> },
    Ready(ImportPreview),
    Failed(String),
    /// An image is waiting for the add-on that lets the local model read it.
    NeedsVision { path: PathBuf, add_on: Model },
}

/// A replay on screen. It only moves the ink: the meeting itself is not touched, so leaving puts
/// everything back.
struct Replaying {
    player: Player,
    /// Every board's steps as the meeting took them, each with its board, so the replay turns as it did.
    timeline: Vec<(usize, serde_json::Value)>,
    /// The timeline's steps alone, which time the replay.
    steps: Vec<serde_json::Value>,
    /// The board the replay is showing.
    shown: usize,
    /// Where things were placed, and how the board was being viewed, before it began.
    memory: serde_json::Value,
    view: (Option<View>, bool),
    next_at: Instant,
    slider: Entity<SliderState>,
    _slider_moved: gpui_kit::Subscription,
}

enum Exported {
    Saved { path: PathBuf, text_left_out: bool },
    Failed(String),
}

/// A summary being written on another thread.
struct SummaryJob {
    cancel: Arc<AtomicBool>,
    updates: Receiver<SummaryUpdate>,
    /// What it is being made from, stored with the result.
    source: String,
    /// Text already there, or edited meanwhile, is never overwritten without asking.
    had_text: bool,
    edits: u64,
    step: Option<(usize, usize)>,
}

enum SummaryUpdate {
    Step(usize, usize),
    Finished(Result<String, String>),
}

struct SummaryPanel {
    editor: Entity<TextareaState>,
    editing: bool,
    job: Option<SummaryJob>,
    /// A generated draft waiting for the choice to replace or keep. Text, then its source.
    draft: Option<(String, String)>,
    /// What "undo" brings back after deleting.
    cleared: Option<(String, Option<String>)>,
    edits: u64,
    note: Option<String>,
    /// The fingerprint of the meeting now: meeting, its change count, then the value.
    current: Option<(String, u64, String)>,
}

impl SummaryPanel {
    fn reset(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.editing = false;
        self.draft = None;
        self.cleared = None;
        self.note = None;
        self.edits += 1;
    }
}

#[derive(Clone)]
enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing { release: Release, progress: Option<Progress> },
    /// Installed; the next start runs it. Never restarted unasked, as a meeting may be under way.
    ReadyToRestart(Release),
    Failed(String),
}

enum UpdateEvent {
    Checked(Result<Option<Release>, String>),
    Progress(Progress),
    Installed(Result<(), String>),
}

/// A model being downloaded.
struct Download {
    id: &'static str,
    label: &'static str,
    cancel: Arc<AtomicBool>,
    updates: Receiver<DownloadUpdate>,
    progress: Option<Progress>,
}

enum DownloadUpdate {
    Progress(Progress),
    Finished(Result<Outcome, String>),
}

struct SettingsForm {
    /// How names are written, as one line of entries apart by semicolons.
    glossary: Entity<InputState>,
    base_url: Entity<InputState>,
    model: Entity<InputState>,
    key: Entity<InputState>,
    remote: bool,
    /// How messy the board's hand is.
    messy: Entity<SliderState>,
    saved: Settings,
    note: Option<String>,
    probing: Option<Receiver<Result<u128, String>>>,
}

struct Workbench {
    ink: Rc<RefCell<Ink>>,
    pipeline: Pipeline,
    input: Entity<InputState>,
    focus: FocusHandle,
    live: Live,
    /// None for a sample or a measurement, which must not touch real meetings.
    store: Option<Store>,
    save: SaveState,
    saving: Option<(Receiver<Result<u64, String>>, u64)>,
    saved_changes: u64,
    seen_changes: u64,
    last_change: Instant,
    last_attempt: Option<Instant>,
    panel: Panel,
    settings_tab: SettingsTab,
    library: Vec<Info>,
    /// Meetings kept at the top of the list.
    pinned: Vec<String>,
    /// The meeting just taken off the list, which can still come back: its id and name.
    deleted_meeting: Option<(String, String)>,
    /// What can be listened to, as of opening Settings.
    inputs: Vec<boardcast_audio::Input>,
    /// The label being written for the selected thing on the board.
    label_input: Entity<InputState>,
    /// What the meeting list is narrowed to.
    library_query: Entity<InputState>,
    form: SettingsForm,
    summary: SummaryPanel,
    replay: Option<Replaying>,
    import: ImportState,
    arxiv_input: Entity<InputState>,
    /// Which of a read document's pages to import, as "1-5, 8"; empty for all.
    import_pages: Entity<InputState>,
    /// Who is speaking now, as far as anyone knows: lines with no voiceprint and typed lines go to them.
    speaker: Option<String>,
    /// The speaker being looked at in the speakers panel, and the name being written for them.
    speaker_chosen: Option<String>,
    /// The transcript line whose speaker was clicked, which can be given to someone else alone.
    speaker_line: Option<usize>,
    speaker_name: Entity<InputState>,
    /// The meeting's name while it is being changed in the title bar.
    title_input: Entity<InputState>,
    editing_title: bool,
    /// What recognizes speech, once the ear is ready.
    hearing: Option<String>,
    /// The system's recognizer, where there is one to choose.
    speech_helper: Option<PathBuf>,
    /// When the microphone went on, so the title bar can count the time.
    listening_since: Option<Instant>,
    clock_second: u64,
    preparing: Option<Preparing>,
    /// Whether the transcript is out. Other panels always show, and close back to it.
    transcript_open: bool,
    /// The speaker under the pointer, whose contributions stand out on the board.
    speaker_hovered: Option<String>,
    transcript_scroll: gpui_kit::ScrollHandle,
    /// The line just jumped to, shown for a moment.
    jumped_line: Option<(usize, Instant)>,
    /// Meeting search, while it is turned on and its model is here.
    search: Option<MeetingSearch>,
    search_tried: bool,
    typing: Option<Typing>,
    pdfium: PathBuf,
    /// Where the stroke data is, for work that needs its own copy.
    hanzi: HanziSource,
    exporting: Option<ExportKind>,
    exported: Option<Exported>,
    /// Where the last file went, to start the next save there.
    export_dir: Option<PathBuf>,
    /// Write the summary once the script has finished, for testing end to end.
    summarize_when_done: bool,
    /// A model given on the command line outranks the settings.
    forced_backend: bool,
    models: PathBuf,
    binaries: PathBuf,
    download: Option<Download>,
    update: UpdateState,
    silent_check: bool,
    update_events: Option<Receiver<UpdateEvent>>,
    /// The folder this copy is installed in; updates only apply to an installed copy.
    prefix: Option<PathBuf>,
    /// When to look for an update without being asked.
    check_at: Option<Instant>,
    data: PathBuf,
    /// The discrete graphics memory, which decides the recommended model.
    vram_mib: u64,
    /// Nothing can draw yet, so opening the app leads to choosing a model.
    needs_model: bool,
    notice: Option<String>,
    /// What the board model is doing, shown at the board's corner while it matters.
    status: ModelStatus,
    /// What recognition wrote for the line being added, when the glossary rewrote it.
    heard: Option<String>,
    /// Another board a line seemed to be about, offered rather than turned to: its place, number and title.
    page_hint: Option<(usize, u32, String)>,
    /// Lines left after turning to a board before another is offered, so the meeting does not bounce.
    page_quiet: u8,
    /// A topic the talk moved on to while the board was already full, offered a board of its own.
    topic_hint: Option<(String, String)>,
    /// Topics offered a board once, never again.
    topics_offered: std::collections::HashSet<String>,
    /// Replies since the topic was offered; left alone, the offer goes.
    topic_hint_age: u8,
    level: f32,
    next_batch: u64,
    exit_when_done: bool,
    /// A failed save blocked closing once; the next attempt closes anyway.
    close_refused: bool,
}

/// Where the graphics driver keeps the shaders it compiles for the local model, beside the models.
fn shader_dir(models: &std::path::Path) -> PathBuf {
    models.parent().unwrap_or(models).join("shader-cache")
}

/// Left beside the shaders once a start has got all the way through. The driver writes into the folder
/// from the moment it starts, so what is there says nothing about whether it finished.
const SHADERS_READY: &str = "ready";

fn dir_size(dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// The local model getting ready: a page says so, as a first start can take half a minute.
struct Preparing {
    started: Instant,
    /// No start has finished here yet, so the driver has shaders to compile.
    first_time: bool,
    done: u64,
    total: u64,
    /// The user chose to carry on without waiting.
    dismissed: bool,
}

impl Preparing {
    fn showing(&self) -> bool {
        !self.dismissed && (self.first_time || self.started.elapsed() > PREPARING_PAGE_AFTER)
    }

    /// How far along, from 0 to 1. Compiling reports nothing, so until tokens are counted this is an
    /// estimate that keeps moving but slows down, and it never claims to be nearly done.
    fn fraction(&self) -> f32 {
        if self.done > 0 && self.total > 0 {
            return ESTIMATED_SHARE + (1.0 - ESTIMATED_SHARE) * self.done as f32 / self.total as f32;
        }
        ESTIMATED_SHARE * (1.0 - (-self.started.elapsed().as_secs_f32() / 15.0).exp())
    }
}

fn backend_from(settings: &Settings, local_only: bool, binaries: &std::path::Path, models: &std::path::Path) -> Backend {
    if settings.enabled && settings.is_complete() && !local_only {
        return Backend::Ready(Provider::Remote { base_url: settings.base_url.clone(), key: settings.api_key.clone(), model: settings.model.clone() });
    }
    match boardcast_models::current(models) {
        Some(model) => Backend::ManagedLocal(Arc::new(LocalServer { binaries: binaries.to_path_buf(), model, shaders: Some(shader_dir(models)) })),
        None => Backend::NoModel,
    }
}

/// An image as the screen takes it: blue first, with alpha left straight.
fn page_render_image(bytes: &[u8]) -> Result<Arc<RenderImage>, String> {
    let mut picture = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    for p in picture.pixels_mut() {
        p.0.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new([image::Frame::new(picture)])))
}

const IMAGE_TYPES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

/// Start a local model that can see, have it read the blocks of the image, and word what happened.
fn read_image(path: &std::path::Path, binaries: &std::path::Path, model: &std::path::Path, projector: &std::path::Path, english: bool, cancel: &AtomicBool, tx: &std::sync::mpsc::Sender<ImportUpdate>) -> anyhow::Result<ImportPreview> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let _ = tx.send(ImportUpdate::Progress(if english { "Starting the local model to read the image…".to_string() } else { "正在启动本地模型来读图…".to_string() }));
    let shaders = model.parent().map(shader_dir);
    let server = VisionServer::start(binaries, model, projector, shaders.as_deref())?;
    let imported = boardcast_import::image::import_image(&bytes, english, cancel, &mut |png| server.read(png), &mut |done, total| {
        let _ = tx.send(ImportUpdate::Progress(if english { format!("Reading {name}: block {done}/{total}") } else { format!("正在读 {name}：{done}/{total} 块") }));
    })?;
    let limit = boardcast_import::image::MAX_AREAS;
    let mut details = vec![if english { format!("Found {} regions; recognized text in {}.", imported.found, imported.read) } else { format!("找到 {} 个区域，其中 {} 个识别到了文字。", imported.found, imported.read) }];
    if imported.capped {
        details.push(if english { format!("Only the first {limit} regions were read; the full image is retained.") } else { format!("只读了前 {limit} 个区域，完整图片仍然保留。") });
    }
    if imported.read == 0 {
        details.push(if english { "No text was recognized. The original image will still be available on the board.".to_string() } else { "没有识别到文字。原图仍会显示在板上。".to_string() });
    }
    let read = imported.read;
    let word = if read == 1 { "text block" } else { "text blocks" };
    let note = match (english, imported.capped) {
        (true, false) => format!("Imported {name} ({read} {word} read)"),
        (true, true) => format!("Imported {name} ({read} {word} read, too many blocks, so only some were read)"),
        (false, false) => format!("导入了 {name}（读出 {read} 块文字）"),
        (false, true) => format!("导入了 {name}（读出 {read} 块文字，块太多只读了一部分）"),
    };
    let ready = match (english, read > 0) {
        (true, true) => format!("{name} is ready. Try \"circle this\" during the meeting"),
        (true, false) => format!("No text found in {name}; it can only be used as a background"),
        (false, true) => format!("{name} 导入好了，开会时可以说「把这里圈出来」"),
        (false, false) => format!("{name} 里没有读出文字，只能当背景用"),
    };
    Ok(ImportPreview { name, pages: vec![imported.page], title: None, note, details, ready })
}

/// Download if need be, read the file, and word what happened.
fn read_import(source: ImportSource, vision: Option<Vision>, library: &std::path::Path, english: bool, cancel: &AtomicBool, tx: &std::sync::mpsc::Sender<ImportUpdate>) -> anyhow::Result<ImportPreview> {
    let (name, bytes) = match source {
        ImportSource::Image { path, binaries, model, projector } => return read_image(&path, &binaries, &model, &projector, english, cancel, tx),
        ImportSource::File(path) => {
            let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
            (name, std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?)
        }
        ImportSource::Arxiv(id) => {
            let _ = tx.send(ImportUpdate::Progress(if english { format!("Downloading {id} from arXiv…") } else { format!("正在从 arXiv 下载 {id}…") }));
            (format!("arXiv {id}"), boardcast_import::arxiv::download(&id, cancel)?)
        }
    };
    let mut imported = boardcast_import::pdf::import_pdf(&bytes, library, english, cancel, &mut |done, total| {
        let _ = tx.send(ImportUpdate::Progress(if english { format!("Reading {name}: page {done}/{total}") } else { format!("正在读 {name}：{done}/{total} 页") }));
    })?;
    // Pages with no text layer are pictures of text: where the local model can see, it reads them, so they
    // can be pointed at, marked and searched like the rest. Each keeps its page number.
    let mut read_scanned = 0;
    if let Some(vision) = vision.filter(|_| !imported.scanned_pages.is_empty()) {
        let _ = tx.send(ImportUpdate::Progress(if english { "Starting the local model to read the scanned pages…".to_string() } else { "正在启动本地模型来读扫描页…".to_string() }));
        let shaders = vision.model.parent().map(shader_dir);
        let server = VisionServer::start(&vision.binaries, &vision.model, &vision.projector, shaders.as_deref())?;
        let count = imported.scanned_pages.len();
        for (k, &i) in imported.scanned_pages.clone().iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                anyhow::bail!("cancelled");
            }
            let page = &mut imported.pages[i];
            let number = page.graph.extra.get("underlay").and_then(|u| u.get("no")).cloned();
            let read = boardcast_import::image::import_image(&page.jpeg, english, cancel, &mut |png| server.read(png), &mut |done, total| {
                let _ = tx.send(ImportUpdate::Progress(if english { format!("Reading scanned page {}/{count}: block {done}/{total}", k + 1) } else { format!("正在读扫描页 {}/{count}：{done}/{total} 块", k + 1) }));
            })?;
            if read.read > 0 {
                *page = read.page;
                if let (Some(number), Some(underlay)) = (number, page.graph.extra.get_mut("underlay")) {
                    underlay["no"] = number;
                }
                read_scanned += 1;
            }
        }
    }
    let plural = |n: usize, word: &str| if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") };
    let mut details = vec![];
    if imported.refs_skipped > 0 {
        details.push(if english { format!("{} of references skipped", plural(imported.refs_skipped, "page")) } else { format!("参考文献 {} 页没建板", imported.refs_skipped) });
    }
    if imported.unread > 0 {
        details.push(if english { format!("board limit reached, last {} not imported", plural(imported.unread, "page")) } else { format!("板到上限了，最后 {} 页没导", imported.unread) });
    }
    if read_scanned > 0 {
        details.push(if english { format!("{} without a text layer read by the local model", plural(read_scanned, "page")) } else { format!("{} 页没有文字层的扫描页由本地模型读出了文字", read_scanned) });
    }
    if imported.scanned > read_scanned {
        // Talk is matched against a page's words, so these can only be turned to by number.
        let left = imported.scanned - read_scanned;
        details.push(if english { format!("{} without a text layer, imported as images and turned to only by number; a local model with its vision add-on can read them", plural(left, "page")) } else { format!("{left} 页没有文字层，整页算一张图，只能按页码翻到；有看图组件的本地模型可以读出其中的文字") });
    }
    let boards = imported.pages.len();
    let note = if english {
        let extra = if details.is_empty() { String::new() } else { format!("; {}", details.join("; ")) };
        format!("Imported {name} ({}, {}{extra})", plural(imported.total, "page"), plural(boards, "board"))
    } else {
        let extra = if details.is_empty() { String::new() } else { format!("，{}", details.join("，")) };
        format!("导入了 {name}（共 {} 页，建了 {boards} 张板{extra}）", imported.total)
    };
    let ready = match (imported.paper, english) {
        (true, true) => format!("{name} is ready: split by sections, figures and equations. Try \"here in figure 3\" or \"equation 5 is wrong\""),
        (true, false) => format!("{name} 导入好了：按论文的章节、图表、公式切好了，可以说「图 3 这里」「公式 5 有问题」"),
        (false, true) => format!("{name} is ready: one board per page. Try \"circle this paragraph\""),
        (false, false) => format!("{name} 导入好了：每页一张板，开会时可以说「把这一段圈出来」"),
    };
    Ok(ImportPreview { name, pages: imported.pages, title: imported.title, note, details, ready })
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn spoken(lines: &[Utterance]) -> Vec<(Option<String>, String)> {
    lines.iter().map(|l| (l.speaker.clone(), l.text.clone())).collect()
}

fn fingerprint_of(live: &Live) -> String {
    summary::fingerprint(&live.boards(), &Line::spoken(&live.lines))
}

/// What is still to happen to a line, said under it; a line on the board needs nothing.
fn line_note(line: &serde_json::Value, model_loading: bool) -> String {
    if line["failed"].as_bool().unwrap_or(false) {
        tr("Not drawn", "没有画上").to_string()
    } else if line["ms"].is_null() {
        if model_loading { tr("Waiting for the model…", "等模型就绪后上板…") } else { tr("Drawing…", "正在画…") }.to_string()
    } else {
        String::new()
    }
}

/// How long hearing and drawing a line took, for whoever hovers to ask.
fn line_timing(line: &serde_json::Value) -> Option<String> {
    let mut parts = vec![];
    if let Some(ms) = line["heard"]["asrMs"].as_u64() {
        parts.push(format!("{} {ms} ms", tr("Heard in", "识别")));
    }
    if let Some(ms) = line["ms"].as_u64().filter(|ms| *ms > 0) {
        parts.push(format!("{} {ms} ms", tr("drawn in", "上板")));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

impl Workbench {
    /// A line was just added: type it in if it was heard, and keep the newest line in view unless the reader
    /// has scrolled back through the meeting.
    fn line_arrived(&mut self, animate: bool) {
        let (offset, max) = (f32::from(self.transcript_scroll.offset().y), f32::from(self.transcript_scroll.max_offset().y));
        let follow = -offset >= max - 24.0;
        self.typing = Some(Typing { line: self.live.lines.len().saturating_sub(1), started: Instant::now(), animate, follow });
    }

    fn session(&self) -> &boardcast_agent::Session {
        self.pipeline.session()
    }

    fn editable(&self) -> bool {
        self.live.read_only.is_none() && self.replay.is_none()
    }

    /// Turn the microphone on or off, counting the time it was on towards the meeting.
    fn toggle_mic(&mut self) {
        let was_listening = self.pipeline.listening();
        self.pipeline.toggle_mic(Some(self.form.saved.input.clone()).filter(|i| !i.is_empty()));
        if !was_listening {
            self.listening_since = Some(Instant::now());
        } else if let Some(since) = self.listening_since.take() {
            self.live.add_recorded(since.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Put the listening so far into the meeting before it is saved, and go on counting from here.
    fn count_listening(&mut self) {
        if let Some(since) = self.listening_since.replace(Instant::now()) {
            self.live.add_recorded(since.elapsed().as_secs_f64() * 1000.0);
        } else {
            self.listening_since = None;
        }
    }

    /// The time the microphone has been on for this meeting, counting now.
    fn recorded(&self) -> Duration {
        Duration::from_millis(self.live.recorded_ms() as u64) + self.listening_since.map_or(Duration::ZERO, |since| since.elapsed())
    }

    fn edit_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let title = self.live.title().to_string();
        self.title_input.update(cx, |state, cx| {
            state.set_value(title, window, cx);
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        self.editing_title = true;
        cx.notify();
    }

    fn finish_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.editing_title) {
            return;
        }
        let title = self.title_input.read(cx).value().trim().to_string();
        if !title.is_empty() && title != self.live.title() {
            self.live.set_title(&title);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Hear with Apple's recognizer or the speech models, in `language` for Apple's.
    fn choose_speech(&mut self, system: bool, language: Option<&str>, cx: &mut Context<Self>) {
        let saved = &mut self.form.saved;
        saved.system_speech = system;
        if let Some(language) = language {
            saved.speech_language = language.to_string();
        }
        if let Err(e) = saved.save() {
            self.form.note = Some(format!("{e:#}"));
        }
        self.pipeline.set_speech(Speech { helper: self.speech_helper.clone(), system, language: saved.speech_language.clone() });
        self.hearing = None;
        cx.notify();
    }

    fn dirty(&self) -> bool {
        self.store.is_some() && self.live.read_only.is_none() && self.live.has_content() && self.live.changes != self.saved_changes
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        {
            let mut ink = self.ink.borrow_mut();
            let before = (ink.base.level, ink.detail.as_ref().map(|(t, c)| (t.level, *c)));
            ink.maintain_sharpness();
            changed |= (ink.base.level, ink.detail.as_ref().map(|(t, c)| (t.level, *c))) != before;
            // Hints go to the model with the next line it is asked about.
            if let Some(hint) = ink.hints.take().filter(|_| self.replay.is_none()) {
                self.session().set_hint(hint);
            }
        }
        if let Some(since) = self.listening_since {
            let second = since.elapsed().as_secs();
            changed |= std::mem::replace(&mut self.clock_second, second) != second;
        }
        while let Ok(event) = self.pipeline.events.try_recv() {
            changed = true;
            match event {
                Event::Agent(Update::Loading) => {
                    self.status = ModelStatus::Loading;
                    self.preparing = Some(Preparing { started: Instant::now(), first_time: !shader_dir(&self.models).join(SHADERS_READY).exists(), done: 0, total: 0, dismissed: false });
                }
                Event::Agent(Update::Preparing { done, total }) => {
                    if let Some(preparing) = &mut self.preparing {
                        (preparing.done, preparing.total) = (done, total);
                    }
                }
                Event::Agent(Update::Ready) => {
                    if self.preparing.take().is_some() {
                        let _ = std::fs::write(shader_dir(&self.models).join(SHADERS_READY), "");
                    }
                    self.status = ModelStatus::Quiet;
                    self.needs_model = false;
                }
                Event::Agent(Update::NoModel) => {
                    self.preparing = None;
                    self.status = ModelStatus::NoModel;
                    self.needs_model = true;
                    self.panel = Panel::Settings;
                }
                // Rises at once and falls away gently, so the bar reads as speech rather than flicker.
                Event::Level(peak) => self.level = boardcast_audio::loudness(peak).max(self.level * 0.9),
                Event::Heard { meeting, speaker, text, asr_ms } => {
                    if meeting == self.session().meeting() {
                        self.live.add_line(speaker.as_deref(), &text, asr_ms);
                        self.line_arrived(true);
                    }
                }
                Event::Spoken { meeting, sentence } => {
                    if meeting == self.session().meeting() {
                        let speaker = self.who_spoke(sentence.voiceprint.as_deref(), sentence.seconds);
                        let mut sentence = sentence;
                        sentence.text = self.as_glossary_writes(&sentence.text);
                        self.live.add_line(speaker.as_deref(), &sentence.text, Some(sentence.asr_ms));
                        self.line_arrived(true);
                        self.keep_heard_text();
                        if self.follow_pages(&sentence.text, cx) {
                            continue;
                        }
                        let renames = self.session().renames_seen();
                        let line = Utterance { speaker, text: sentence.text, pointing: self.take_pointing() };
                        let _ = self.pipeline.said.send(Said { line, ended: sentence.ended, asr_ms: Some(sentence.asr_ms), meeting, renames });
                    }
                }
                Event::Agent(Update::Drawn { meeting, graph, ops, lines, asr_ms, llm_ms, ended }) => {
                    if meeting != self.session().meeting() {
                        continue;
                    }
                    self.exit_replay(cx);
                    println!("{{\"ops\":{},\"board\":{}}}", serde_json::Value::from(ops.clone()), serde_json::Value::from(graph.describe()));
                    self.next_batch += 1;
                    let id = self.next_batch;
                    let arrived = Instant::now();
                    let text = lines.last().map_or(String::new(), |l| l.text.clone());
                    let mut ink = self.ink.borrow_mut();
                    let strokes = ink.update(&graph, id);
                    if strokes > 0 {
                        ink.timings.push(Timing { id, text, asr_ms, llm_ms, batch: lines.len(), ended, arrived, first_ink: None, strokes });
                    }
                    let memory = ink.board.snapshot();
                    drop(ink);
                    self.offer_topic_board(&graph);
                    self.live.model_drew(&spoken(&lines), graph, memory, &ops, llm_ms);
                    self.status = ModelStatus::Quiet;
                }
                Event::Agent(Update::Failed { meeting, message, lines }) => {
                    if meeting != self.session().meeting() {
                        continue;
                    }
                    eprintln!("model failed: {message}");
                    self.live.model_failed(&spoken(&lines));
                    self.notice = Some(format!("{}{message}", tr("The model did not answer: ", "模型没有回应：")));
                }
                Event::Hearing { with, note } => {
                    if let Some(note) = note {
                        self.notice = Some(format!("{}{note}", tr("Apple's speech recognition did not start, so the speech models hear instead: ", "Apple 语音识别没能启动，改用语音模型：")));
                    }
                    self.hearing = Some(with);
                }
                Event::Agent(Update::Broken(message)) | Event::Failed(message) => {
                    self.preparing = None;
                    eprintln!("failed: {message}");
                    self.notice = Some(message);
                }
                Event::Done => {
                    println!("{{\"done\":true,\"rss_mb\":{}}}", rss_mb());
                    if self.summarize_when_done {
                        self.panel = Panel::Summary;
                        self.make_summary(cx);
                    } else if self.exit_when_done {
                        self.exit_soon(cx);
                    }
                }
            }
        }
        if let Some((line, at)) = self.jumped_line {
            if at.elapsed() > JUMP_SHOWN {
                self.jumped_line = None;
                changed = true;
            } else if at.elapsed() < JUMP_SCROLLING {
                // A list scrolls only once it has been laid out, which the transcript may not have
                // been if another panel was showing.
                self.transcript_scroll.scroll_to_item(line);
                changed = true;
            }
        }
        if let Some(typing) = &self.typing {
            if typing.follow {
                self.transcript_scroll.scroll_to_bottom();
            }
            let len = self.live.lines.get(typing.line).and_then(|l| l["text"].as_str()).map_or(0, |t| t.chars().count());
            // Kept a little past the last character, so the list is laid out before it stops following.
            let typed = !typing.animate || typing.chars() >= len;
            if typed && typing.started.elapsed() > JUMP_SCROLLING {
                self.typing = None;
            }
            changed = true;
        }
        // The bar has to keep moving even while nothing is reported.
        changed |= self.preparing.as_ref().is_some_and(Preparing::showing);
        self.pump_probe(&mut changed);
        self.pump_import(&mut changed);
        self.pump_replay(&mut changed, cx);
        self.pump_summary(&mut changed, cx);
        self.pump_download(&mut changed);
        self.pump_search(&mut changed);
        self.pump_update(&mut changed);
        changed |= self.autosave();
        if changed {
            cx.notify();
        }
    }

    /// Give the saves and the screen a moment, then leave. Only used by scripted runs.
    fn exit_soon(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(6)).await;
            let saved = this.update(cx, |this, _| this.flush());
            println!("{{\"exit_save_ok\":{}}}", matches!(saved, Ok(Ok(()))));
            std::process::exit(0);
        })
        .detach();
    }

    fn pump_summary(&mut self, changed: &mut bool, cx: &mut Context<Self>) {
        let Some(job) = &mut self.summary.job else { return };
        let mut finished = None;
        loop {
            match job.updates.try_recv() {
                Ok(SummaryUpdate::Step(done, total)) => {
                    job.step = Some((done, total));
                    *changed = true;
                }
                Ok(SummaryUpdate::Finished(result)) => {
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err("the summary did not finish".to_string()));
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        let Some(result) = finished else { return };
        let job = self.summary.job.take().expect("checked above");
        *changed = true;
        match result {
            Ok(text) => {
                println!("{{\"summary\":{}}}", serde_json::Value::from(text.clone()));
                if job.had_text || !self.live.summary().trim().is_empty() || self.summary.edits != job.edits {
                    self.summary.draft = Some((text, job.source));
                } else {
                    self.live.set_summary(&text, Some(&job.source));
                    self.summary.cleared = None;
                }
            }
            Err(error) => {
                eprintln!("summary failed: {error}");
                self.summary.note = Some(format!("{}{error}", tr("Could not write the summary: ", "生成摘要失败：")));
            }
        }
        if self.summarize_when_done && self.exit_when_done {
            self.exit_soon(cx);
        }
    }

    fn make_summary(&mut self, cx: &mut Context<Self>) {
        if !self.editable() || self.summary.job.is_some() || self.summary.draft.is_some() {
            return;
        }
        let provider = match self.session().provider() {
            Ok(provider) => provider,
            Err(error) => {
                self.summary.note = Some(format!("{}{error:#}", tr("Could not write the summary: ", "生成摘要失败：")));
                cx.notify();
                return;
            }
        };
        let boards = self.live.boards();
        let lines = Line::spoken(&self.live.lines);
        let source = summary::fingerprint(&boards, &lines);
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, updates) = channel();
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let result = summary::summarize(&provider, &boards, &lines, &stop, &mut |Step::Writing { done, total }| {
                let _ = tx.send(SummaryUpdate::Step(done, total));
            });
            let _ = tx.send(SummaryUpdate::Finished(result.map_err(|e| format!("{e:#}"))));
        });
        self.summary.note = None;
        self.summary.job = Some(SummaryJob { cancel, updates, source, had_text: !self.live.summary().trim().is_empty(), edits: self.summary.edits, step: None });
        cx.notify();
    }

    /// A request already sent cannot be recalled, so whatever it returns is simply dropped with the job.
    fn stop_summary(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = self.summary.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    fn edit_summary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.summary.editing {
            self.summary.editing = false;
            window.focus(&self.focus, cx);
        } else if self.editable() {
            let text = self.live.summary().to_string();
            self.summary.editor.update(cx, |state, cx| state.set_value(text, window, cx));
            self.summary.editing = true;
            self.summary.editor.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    /// Typing goes straight into the meeting, so nothing typed can be lost by leaving the panel.
    fn summary_typed(&mut self, cx: &mut Context<Self>) {
        if !self.summary.editing || !self.editable() {
            return;
        }
        let text = self.summary.editor.read(cx).value().to_string();
        if text != self.live.summary() {
            let source = self.live.summary_source().map(str::to_string).unwrap_or_else(|| fingerprint_of(&self.live));
            self.live.set_summary(&text, Some(&source));
            self.summary.edits += 1;
            self.summary.cleared = None;
            cx.notify();
        }
    }

    fn tick_todo(&mut self, line: usize, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let text = summary::toggle_todo(self.live.summary(), line);
        let source = self.live.summary_source().map(str::to_string);
        self.live.set_summary(&text, source.as_deref());
        self.summary.edits += 1;
        cx.notify();
    }

    fn clear_summary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        self.summary.cleared = Some((self.live.summary().to_string(), self.live.summary_source().map(str::to_string)));
        self.summary.editing = false;
        self.summary.edits += 1;
        self.live.set_summary("", None);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn undo_clear(&mut self, cx: &mut Context<Self>) {
        if let Some((text, source)) = self.summary.cleared.take() {
            if self.editable() {
                self.live.set_summary(&text, source.as_deref());
                self.summary.edits += 1;
            }
        }
        cx.notify();
    }

    fn use_draft(&mut self, cx: &mut Context<Self>) {
        if let Some((text, source)) = self.summary.draft.take() {
            if self.editable() {
                self.live.set_summary(&text, Some(&source));
                self.summary.edits += 1;
                self.summary.editing = false;
                self.summary.cleared = None;
            }
        }
        cx.notify();
    }

    fn copy_summary(&mut self, text: String, what: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
        self.summary.note = Some(what.to_string());
        cx.notify();
    }

    fn pump_replay(&mut self, changed: &mut bool, cx: &mut Context<Self>) {
        let Some(replay) = &self.replay else { return };
        if replay.player.playing && Instant::now() >= replay.next_at {
            let next = replay.player.at + 1;
            self.seek_replay(next, cx);
            *changed = true;
        }
    }

    fn wait_for_next_step(&mut self) {
        if let Some(replay) = &mut self.replay {
            replay.next_at = Instant::now() + Duration::from_secs_f64(replay.player.wait_ms(&replay.steps) / 1000.0);
        }
    }

    fn start_replay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.replay.is_some() {
            return;
        }
        let waiting = self.live.lines.iter().any(|l| l["ms"].is_null() && !l["manual"].as_bool().unwrap_or(false));
        if self.pipeline.listening() || waiting {
            self.notice = Some(tr("Can't replay while listening or while the model is drawing.", "收音中或模型还在画时不能回放。").into());
            cx.notify();
            return;
        }
        let timeline = boardcast_board::steps::timeline(&self.live.board_steps());
        let steps: Vec<serde_json::Value> = timeline.iter().map(|(_, s)| s.clone()).collect();
        let total = steps.len();
        if total == 0 {
            self.notice = Some(tr("This meeting has no board history to replay.", "这场会没有可以回放的记录。").into());
            cx.notify();
            return;
        }
        let slider = cx.new(|_| SliderState::new().min(0.0).max(total as f32).step(1.0).default_value(0.0));
        let slider_moved = cx.subscribe(&slider, |this: &mut Self, _, event: &SliderEvent, cx| {
            if let SliderEvent::Change(value) = event {
                this.seek_replay(value.end().round() as usize, cx);
            }
        });
        let mut ink = self.ink.borrow_mut();
        let (memory, view) = (ink.board.snapshot(), (ink.view, ink.reading));
        ink.open(&Graph::default(), &serde_json::Value::Null);
        drop(ink);
        self.notice = None;
        self.panel = Panel::Transcript;
        let shown = timeline.first().map_or(self.live.page(), |(b, _)| *b);
        self.replay = Some(Replaying { player: Player::new(total), timeline, steps, shown, memory, view, next_at: Instant::now(), slider, _slider_moved: slider_moved });
        self.show_board_image(shown);
        self.wait_for_next_step();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn seek_replay(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(replay) = &mut self.replay else { return };
        let Some(moved) = replay.player.seek(at) else { return };
        let (board, count) = boardcast_board::steps::board_at(&replay.timeline, replay.player.at);
        let own: Vec<serde_json::Value> = replay.timeline.iter().filter(|(b, _)| *b == board).map(|(_, s)| s.clone()).collect();
        let graph = replay_to(&own, count);
        let turned = board != replay.shown;
        replay.shown = board;
        if turned {
            self.show_board_image(board);
        }
        self.next_batch += 1;
        let batch = self.next_batch;
        let mut ink = self.ink.borrow_mut();
        // Another board is laid out afresh, as it was when the meeting turned to it.
        if moved < 0 || turned {
            // Going back lays out from the start, as new things found their places the first time.
            ink.open(&graph, &serde_json::Value::Null);
        } else {
            // Writing out one step shows it being drawn; a jump just shows where things are.
            ink.instant = moved > 1;
            ink.update(&graph, batch);
        }
        drop(ink);
        self.wait_for_next_step();
        cx.notify();
    }

    fn toggle_replay(&mut self, cx: &mut Context<Self>) {
        let Some(replay) = &mut self.replay else { return };
        if replay.player.toggle() {
            self.ink.borrow_mut().open(&Graph::default(), &serde_json::Value::Null);
        }
        self.wait_for_next_step();
        cx.notify();
    }

    fn exit_replay(&mut self, cx: &mut Context<Self>) {
        let Some(replay) = self.replay.take() else { return };
        let mut ink = self.ink.borrow_mut();
        ink.open(&self.live.graph, &replay.memory);
        (ink.view, ink.reading) = replay.view;
        drop(ink);
        if replay.shown != self.live.page() {
            self.show_page_image();
        }
        cx.notify();
    }

    /// Carry on from this moment on a new board, with things where they are now. The original stays.
    fn fork_replay(&mut self, cx: &mut Context<Self>) {
        let Some(replay) = &self.replay else { return };
        if replay.player.at == 0 || self.live.read_only.is_some() {
            return;
        }
        let when = clock(replay.player.elapsed_ms(&replay.steps));
        // The fork is of the board in view, from as far as that board had come.
        let (board, at) = boardcast_board::steps::board_at(&replay.timeline, replay.player.at);
        self.live.go_to_page(board);
        let memory = self.ink.borrow().board.snapshot();
        let (from, to) = (self.live.page() + 1, self.live.pages() + 1);
        let note = if tr("en", "zh") == "en" { format!("Board {to} forked from board {from} as it was at {when}") } else { format!("从第 {from} 张板回放到 {when} 的那一刻，另起了第 {to} 张板") };
        self.replay = None;
        self.live.fork_page(at, memory, &note);
        let history: Vec<Utterance> = self.live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect();
        self.session().open(self.live.graph.clone(), history);
        self.ink.borrow_mut().open(&self.live.graph, &self.live.memory);
        self.show_page_image();
        cx.notify();
    }

    /// The meeting is captured on the click, so what goes on while the dialog is open stays out of the file.
    fn start_export(&mut self, kind: ExportKind, cx: &mut Context<Self>) {
        if self.exporting.is_some() {
            return;
        }
        let images: Vec<Option<boardcast_meetings::PageImage>> = self
            .live
            .board_images()
            .iter()
            .map(|r| {
                r.as_ref().and_then(|r| match &self.store {
                    Some(store) => store.page_image(self.live.id(), r).ok(),
                    None => boardcast_meetings::decode_data_url(r).ok(),
                })
            })
            .collect();
        let frozen = boardcast_export::freeze(&self.live, tr("en", "zh") == "en", &mut self.ink.borrow_mut().glyphs, self.hanzi.clone(), &images);
        let directory = self.export_dir.clone().unwrap_or_else(|| {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map_or_else(|| self.data.clone(), PathBuf::from);
            Some(home.join("Documents")).filter(|d| d.is_dir()).unwrap_or(home)
        });
        let answer = cx.prompt_for_new_path(&directory, Some(&boardcast_export::file_name(kind)));
        self.exporting = Some(kind);
        self.exported = None;
        cx.spawn(async move |this, cx| {
            let chosen = match answer.await {
                Ok(Ok(chosen)) => chosen,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |this, cx| this.finish_export(Some(Exported::Failed(format!("{error:#}"))), cx));
                    return;
                }
                Err(_) => None,
            };
            let Some(path) = chosen else {
                let _ = this.update(cx, |this, cx| this.finish_export(None, cx));
                return;
            };
            let path = boardcast_export::with_extension(path, kind);
            let target = path.clone();
            let written = cx
                .background_executor()
                .spawn(async move {
                    let output = frozen.render(kind)?;
                    boardcast_export::write_file(&target, &output.bytes)?;
                    anyhow::Ok(output.text_left_out)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let result = match written {
                    Ok(text_left_out) => {
                        this.export_dir = path.parent().map(PathBuf::from);
                        Exported::Saved { path, text_left_out }
                    }
                    Err(error) => {
                        eprintln!("export failed: {error:#}");
                        Exported::Failed(format!("{error:#}"))
                    }
                };
                this.finish_export(Some(result), cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_export(&mut self, result: Option<Exported>, cx: &mut Context<Self>) {
        self.exporting = None;
        self.exported = result;
        cx.notify();
    }

    fn pump_probe(&mut self, changed: &mut bool) {
        let Some(rx) = &self.form.probing else { return };
        match rx.try_recv() {
            Ok(result) => {
                self.form.note = Some(match result {
                    Ok(ms) => format!("{} {ms} ms", tr("Connected in", "连接成功，用时")),
                    Err(e) => format!("{}{e}", tr("Could not connect: ", "连接失败：")),
                });
                self.form.probing = None;
                *changed = true;
            }
            Err(TryRecvError::Disconnected) => self.form.probing = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    fn check_for_update(&mut self) {
        if self.prefix.is_none() || self.update_events.is_some() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(UpdateEvent::Checked(boardcast_update::check().map_err(|e| format!("{e:#}"))));
        });
        self.update_events = Some(rx);
        self.update = UpdateState::Checking;
    }

    fn pump_update(&mut self, changed: &mut bool) {
        if self.check_at.is_some_and(|at| Instant::now() >= at) {
            self.check_at = None;
            self.silent_check = true;
            self.check_for_update();
            *changed = true;
        }
        let Some(rx) = &self.update_events else { return };
        let mut events = vec![];
        let mut ended = false;
        loop {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        for event in events {
            *changed = true;
            match event {
                UpdateEvent::Checked(Ok(Some(release))) => self.update = UpdateState::Available(release),
                UpdateEvent::Checked(Ok(None)) => self.update = UpdateState::UpToDate,
                // A check nobody asked for stays quiet: being offline is not worth a message.
                UpdateEvent::Checked(Err(_)) if self.silent_check => self.update = UpdateState::Idle,
                UpdateEvent::Checked(Err(error)) => self.update = UpdateState::Failed(error),
                UpdateEvent::Progress(p) => {
                    if let UpdateState::Installing { progress, .. } = &mut self.update {
                        *progress = Some(p);
                    }
                }
                UpdateEvent::Installed(Ok(())) => {
                    if let UpdateState::Installing { release, .. } = &self.update {
                        self.update = UpdateState::ReadyToRestart(release.clone());
                    }
                }
                UpdateEvent::Installed(Err(error)) => self.update = UpdateState::Failed(error),
            }
        }
        if ended {
            self.update_events = None;
            self.silent_check = false;
        }
    }

    /// Download and install in the background, so the meeting carries on meanwhile.
    fn install_update(&mut self, cx: &mut Context<Self>) {
        let (UpdateState::Available(release), Some(prefix)) = (self.update.clone(), self.prefix.clone()) else { return };
        if self.update_events.is_some() {
            return;
        }
        let dir = self.data.join("updates");
        let (tx, rx) = channel();
        let package = release.clone();
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let progress = tx.clone();
            let result = boardcast_update::fetch(&package, &dir, &cancel, &mut |p| {
                let _ = progress.send(UpdateEvent::Progress(p));
            })
            .and_then(|archive| {
                let archive = archive.context("the download was cancelled")?;
                let installed = boardcast_update::install(&archive, &package, &prefix);
                // Used or rejected, this package is finished with; a bad one must not be reused.
                let _ = std::fs::remove_file(&archive);
                installed
            });
            let _ = tx.send(UpdateEvent::Installed(result.map_err(|e| format!("{e:#}"))));
        });
        self.update_events = Some(rx);
        self.update = UpdateState::Installing { release, progress: None };
        cx.notify();
    }

    /// Start the new version, once the meeting is safely saved.
    fn restart(&mut self, cx: &mut Context<Self>) {
        let Some(prefix) = self.prefix.clone() else { return };
        if !self.leave() {
            cx.notify();
            return;
        }
        match std::process::Command::new(prefix.join("bin/boardcast")).spawn() {
            Ok(_) => cx.quit(),
            Err(e) => self.update = UpdateState::Failed(format!("{}{e}", tr("Could not start the new version: ", "无法启动新版本："))),
        }
        cx.notify();
    }

    fn pump_download(&mut self, changed: &mut bool) {
        let Some(download) = &mut self.download else { return };
        let mut finished = None;
        while let Ok(update) = download.updates.try_recv() {
            *changed = true;
            match update {
                DownloadUpdate::Progress(progress) => download.progress = Some(progress),
                DownloadUpdate::Finished(result) => finished = Some(result),
            }
        }
        let Some(result) = finished else { return };
        let (id, label) = (download.id, download.label);
        self.download = None;
        match result {
            Ok(Outcome::Finished(_)) => {
                if let Some(model) = boardcast_models::catalog().iter().find(|m| m.id == id) {
                    if let Err(e) = boardcast_models::select(model, &self.models) {
                        self.notice = Some(format!("{e:#}"));
                    }
                }
                // A vision add-on is not a board model, so it leaves that choice alone.
                if boardcast_models::catalog().iter().any(|m| m.id == id) {
                    self.needs_model = false;
                }
                self.form.note = Some(format!("{label} {}", tr("is ready.", "已就绪。")));
                if id == boardcast_models::embedding().id {
                    self.start_search();
                } else {
                    self.apply_backend();
                }
            }
            Ok(Outcome::Cancelled) => self.form.note = Some(tr("Paused. Download again to continue from where it stopped.", "已暂停。再次下载会从中断处继续。").into()),
            Err(error) => self.form.note = Some(error),
        }
    }

    /// Start meeting search if it is on and its model is here, and bring every meeting's index up to date.
    fn start_search(&mut self) {
        let model = boardcast_models::embedding();
        let (Some(store), Some(data)) = (&self.store, self.models.parent()) else { return };
        if self.search.is_some() || !self.form.saved.meeting_search || !boardcast_models::installed(&model, &self.models) {
            return;
        }
        let binaries = self.binaries.clone();
        let spawn: boardcast_search::service::Spawn = Box::new(move |args| boardcast_agent::spawn_tied({
            let mut cmd = boardcast_agent::server_command(&binaries);
            cmd.args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            cmd
        }));
        let (jobs, events) = boardcast_search::service::start(data, self.models.join(&model.file), spawn);
        let all: Vec<String> = store.list().unwrap_or_default().into_iter().filter(|m| m.read_error.is_none()).map(|m| m.id).collect();
        let _ = jobs.send(boardcast_search::service::Job::Index(all));
        self.search = Some(MeetingSearch { jobs, events, indexing: None, asked: String::new(), typed_at: None, ticket: 0, asking: false, hits: vec![], saved: HashMap::new(), error: None });
    }

    /// Turn meeting search on or off. On, it downloads its model first if need be.
    fn set_meeting_search(&mut self, on: bool, cx: &mut Context<Self>) {
        self.form.saved.meeting_search = on;
        if let Err(e) = self.form.saved.save() {
            self.form.note = Some(format!("{e:#}"));
        }
        if on {
            let model = boardcast_models::embedding();
            if boardcast_models::installed(&model, &self.models) {
                self.start_search();
            } else {
                self.start_download(model, cx);
            }
        } else {
            // Dropping the worker's channel stops it, and its model with it.
            self.search = None;
        }
        cx.notify();
    }

    fn pump_search(&mut self, changed: &mut bool) {
        use boardcast_search::service::{Event as SearchEvent, Job};
        // While listening, the CPU is for hearing and the board; the meeting is read again once it stops.
        let listening = self.pipeline.listening();
        let library: Vec<String> = self.library.iter().filter(|m| m.read_error.is_none()).map(|m| m.id.clone()).collect();
        if self.search.is_none() && !self.search_tried {
            // At start, once: search that was turned on comes back on.
            self.search_tried = true;
            self.start_search();
        }
        let Some(search) = &mut self.search else { return };
        while let Ok(event) = search.events.try_recv() {
            *changed = true;
            match event {
                SearchEvent::Indexing { done, total } => search.indexing = (done < total).then_some((done, total)),
                SearchEvent::Indexed { .. } => {}
                SearchEvent::Found { ticket, hits } => {
                    if ticket == search.ticket {
                        search.hits = hits;
                        search.asking = false;
                        search.error = None;
                    }
                }
                SearchEvent::Failed(error) => {
                    search.indexing = None;
                    search.asking = false;
                    search.error = Some(error);
                }
            }
        }
        if !listening {
            let due: Vec<String> = search.saved.iter().filter(|(_, at)| at.elapsed() >= REINDEX_AFTER).map(|(id, _)| id.clone()).collect();
            if !due.is_empty() {
                search.saved.retain(|id, _| !due.contains(id));
                let _ = search.jobs.send(Job::Index(due));
            }
        }
        if search.typed_at.is_some_and(|at| at.elapsed() >= QUERY_AFTER) {
            search.typed_at = None;
            search.ticket += 1;
            search.asking = true;
            let _ = search.jobs.send(Job::Find { text: search.asked.clone(), meetings: library, limit: 8, ticket: search.ticket });
        }
    }

    /// Where meeting search stands, for the settings: downloading, indexing, ready, or what went wrong.
    fn search_status(&self) -> Option<(String, bool)> {
        if !self.form.saved.meeting_search {
            return None;
        }
        let model = boardcast_models::embedding();
        if let Some(download) = self.download.as_ref().filter(|d| d.id == model.id) {
            let share = download.progress.as_ref().map_or(0.0, |p| p.done as f64 / p.total.max(1) as f64);
            return Some((format!("{} {:.0}%", tr("Downloading the model", "正在下载模型"), share * 100.0), false));
        }
        let search = self.search.as_ref()?;
        if let Some(error) = &search.error {
            return Some((error.clone(), true));
        }
        Some(match search.indexing {
            Some((done, total)) => (format!("{} {done} / {total}", tr("Reading meetings", "正在读取会议")), false),
            None => (tr("Ready. Type in the meetings' search box.", "已就绪。在会议页的搜索框里输入即可。").to_string(), false),
        })
    }

    /// The library's query changed: ask again once typing pauses, or clear the results.
    fn search_typed(&mut self, text: &str) {
        let Some(search) = &mut self.search else { return };
        let text = text.trim();
        if text == search.asked {
            return;
        }
        search.asked = text.to_string();
        search.hits.clear();
        search.typed_at = (text.chars().count() >= 2).then(Instant::now);
        search.asking = search.typed_at.is_some();
    }

    /// Open a found passage: its meeting, at its line, its board or the summary.
    fn open_found(&mut self, found: &boardcast_search::service::Found, cx: &mut Context<Self>) {
        use boardcast_search::Source;
        self.open_meeting(&found.meeting, cx);
        if self.live.id() != found.meeting {
            return;
        }
        match &found.source {
            Source::Transcript { first, .. } => {
                self.panel = Panel::Transcript;
                self.transcript_open = true;
                self.jumped_line = Some((*first, Instant::now()));
            }
            Source::Board { page, .. } => {
                if *page != self.live.page() {
                    self.go_to_page(*page, cx);
                }
                self.panel = Panel::Transcript;
            }
            Source::Summary { .. } => self.panel = Panel::Summary,
        }
        cx.notify();
    }

    fn start_download(&mut self, model: Model, cx: &mut Context<Self>) {
        if self.download.is_some() {
            return;
        }
        let (tx, updates) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (flag, dir) = (cancel.clone(), self.models.clone());
        self.download = Some(Download { id: model.id, label: model.label, cancel, updates, progress: None });
        self.form.note = None;
        std::thread::spawn(move || {
            let progress = tx.clone();
            let result = boardcast_models::download(&model, &dir, &flag, &mut |p| {
                let _ = progress.send(DownloadUpdate::Progress(p));
            });
            let _ = tx.send(DownloadUpdate::Finished(result.map_err(|e| format!("{e:#}"))));
        });
        cx.notify();
    }

    fn use_model(&mut self, model: &Model, cx: &mut Context<Self>) {
        match boardcast_models::select(model, &self.models) {
            Ok(()) => {
                self.form.remote = false;
                self.form.saved.enabled = false;
                if let Err(e) = self.form.saved.save() {
                    self.form.note = Some(format!("{e:#}"));
                }
                self.apply_backend();
            }
            Err(e) => self.form.note = Some(format!("{e:#}")),
        }
        cx.notify();
    }

    fn finish_save(&mut self, result: Result<u64, String>, covers: u64) {
        self.last_attempt = Some(Instant::now());
        match result {
            Ok(revision) => {
                self.live.saved(revision);
                self.saved_changes = covers;
                if let Some(search) = &mut self.search {
                    search.saved.insert(self.live.id().to_string(), Instant::now());
                }
                self.save = if self.live.changes == covers { SaveState::Saved } else { SaveState::Dirty };
            }
            Err(error) => {
                eprintln!("save failed: {error}");
                self.save = SaveState::Failed(error);
            }
        }
    }

    /// Save a short while after the last change. Failures stay on screen and are retried, and the
    /// meeting stays open with everything in it.
    fn autosave(&mut self) -> bool {
        let before = self.save.clone();
        if self.live.changes != self.seen_changes {
            self.seen_changes = self.live.changes;
            self.last_change = Instant::now();
            if !matches!(self.save, SaveState::Failed(_) | SaveState::Saving) && self.dirty() {
                self.save = SaveState::Dirty;
            }
        }
        if let Some((rx, covers)) = &self.saving {
            let covers = *covers;
            match rx.try_recv() {
                Ok(result) => {
                    self.saving = None;
                    self.finish_save(result, covers);
                }
                Err(TryRecvError::Disconnected) => {
                    self.saving = None;
                    self.finish_save(Err("the save did not finish".into()), covers);
                }
                Err(TryRecvError::Empty) => return self.save != before,
            }
        }
        let waited = match self.save {
            SaveState::Failed(_) => self.last_attempt.is_none_or(|t| t.elapsed() >= RETRY_SAVE_AFTER),
            _ => self.last_change.elapsed() >= AUTOSAVE_AFTER,
        };
        if self.dirty() && waited {
            self.start_save();
        }
        self.save != before
    }

    fn start_save(&mut self) {
        let Some(store) = self.store.clone() else { return };
        let covers = self.live.changes;
        let (id, snapshot, origin, revision) = (self.live.id().to_string(), self.live.snapshot(), self.live.origin.clone(), self.live.revision);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(store.save(&id, snapshot, &origin, revision).map_err(|e| format!("{e:#}")));
        });
        self.saving = Some((rx, covers));
        if !matches!(self.save, SaveState::Failed(_)) {
            self.save = SaveState::Saving;
        }
    }

    /// Everything on disk before leaving this meeting. On failure the caller must stay.
    fn flush(&mut self) -> Result<(), String> {
        if let Some((rx, covers)) = self.saving.take() {
            let result = rx.recv().unwrap_or_else(|_| Err("the save did not finish".into()));
            self.finish_save(result, covers);
        }
        if self.dirty() {
            let store = self.store.clone().expect("dirty needs a store");
            let covers = self.live.changes;
            let id = self.live.id().to_string();
            let origin = self.live.origin.clone();
            let revision = self.live.revision;
            let result = store.save(&id, self.live.snapshot(), &origin, revision).map_err(|e| format!("{e:#}"));
            self.finish_save(result, covers);
        }
        match &self.save {
            SaveState::Failed(error) if self.dirty() => Err(error.clone()),
            _ => Ok(()),
        }
    }

    fn apply_backend(&self) {
        if !self.forced_backend {
            self.session().set_backend(backend_from(&self.form.saved, self.live.local_only(), &self.binaries, &self.models));
        }
    }

    /// Put a meeting on screen. Anything still in flight for the previous one is discarded.
    fn show(&mut self, live: Live) {
        self.replay = None;
        if self.pipeline.listening() {
            self.toggle_mic();
        }
        self.editing_title = false;
        let history: Vec<Utterance> = live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect();
        self.session().open(live.graph.clone(), history);
        self.session().set_references(references_of(&live.boards()));
        self.ink.borrow_mut().open(&live.graph, &live.memory);
        self.summary.reset();
        self.speaker = None;
        self.speaker_chosen = None;
        self.speaker_line = None;
        self.speaker_hovered = None;
        self.jumped_line = None;
        self.typing = None;
        self.live = live;
        self.live.adopt_summary_source(&fingerprint_of(&self.live));
        self.show_page_image();
        self.saved_changes = self.live.changes;
        self.seen_changes = self.live.changes;
        self.save = SaveState::Idle;
        self.last_attempt = None;
        self.notice = None;
        self.apply_backend();
        if let Some(store) = &self.store {
            let _ = store.remember_opened(self.live.id());
        }
    }

    /// Leave the current meeting only if it is safely saved; otherwise stay and say why.
    fn leave(&mut self) -> bool {
        self.count_listening();
        match self.flush() {
            Ok(()) => true,
            Err(error) => {
                self.notice = Some(format!("{}{error}", tr("Staying on this meeting because it could not be saved: ", "这场会保存失败，先留在这里：")));
                false
            }
        }
    }

    fn new_meeting(&mut self, cx: &mut Context<Self>) {
        if self.leave() {
            self.show(Live::new());
            self.panel = Panel::Transcript;
        }
        cx.notify();
    }

    /// Keep this board's topics as a template, named after the meeting.
    fn save_template(&mut self, cx: &mut Context<Self>) {
        let topics: Vec<String> = self.live.graph.nodes.iter().filter(|n| n.kind == Kind::Topic).map(|n| n.label.clone()).collect();
        if topics.is_empty() {
            return;
        }
        let name = Some(self.live.title().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| topics.join(" · "));
        self.form.saved.templates.retain(|(n, _)| *n != name);
        self.form.saved.templates.push((name, topics));
        if let Err(e) = self.form.saved.save() {
            self.notice = Some(format!("{e:#}"));
        }
        cx.notify();
    }

    fn forget_template(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.form.saved.templates.len() {
            self.form.saved.templates.remove(index);
            if let Err(e) = self.form.saved.save() {
                self.notice = Some(format!("{e:#}"));
            }
        }
        cx.notify();
    }

    /// A new meeting with a template's topics already on the board, for the model to fill.
    fn new_meeting_from(&mut self, topics: Vec<String>, cx: &mut Context<Self>) {
        if self.leave() {
            let mut live = Live::new();
            let ops: String = topics.iter().map(|t| format!("T 「{t}」\n")).collect();
            let mut graph = Graph::default();
            graph.apply(&parse_ops(&ops));
            // Kept so the summary can follow the same structure.
            graph.extra.insert("template".into(), serde_json::json!(topics));
            live.change_board(graph, serde_json::Value::Null);
            self.show(live);
            self.panel = Panel::Transcript;
        }
        cx.notify();
    }

    fn open_meeting(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(store) = self.store.clone() else { return };
        if id != self.live.id() && self.leave() {
            match store.load(id) {
                Ok(Some(loaded)) => {
                    self.show(Live::from_loaded(loaded));
                    self.panel = Panel::Transcript;
                }
                Ok(None) => self.notice = Some(tr("That meeting no longer exists.", "这场会已经不存在了。").into()),
                Err(e) => self.notice = Some(format!("{}{e:#}", tr("Could not open the meeting: ", "打不开这场会："))),
            }
        }
        cx.notify();
    }

    /// A line with its names written as the glossary has them. What was heard is kept for the line.
    fn as_glossary_writes(&mut self, heard: &str) -> String {
        let written = Glossary::parse(&self.form.saved.glossary).correct(heard);
        self.heard = (written != heard).then(|| heard.to_string());
        written
    }

    /// The text recognition gave, on the line just added, when the glossary changed it.
    fn keep_heard_text(&mut self) {
        if let (Some(heard), Some(line)) = (self.heard.take(), self.live.lines.last_mut()) {
            line["rawText"] = serde_json::Value::from(heard);
        }
    }

    /// When the model starts a topic on a board that already holds a good deal, offer the topic a board of
    /// its own. Each topic is offered once, and not just after a turn to another board.
    fn offer_topic_board(&mut self, drawn: &Graph) {
        if self.topic_hint.is_some() {
            self.topic_hint_age += 1;
            if self.topic_hint_age > TOPIC_HINT_REPLIES {
                self.topic_hint = None;
            }
        }
        let before = &self.live.graph;
        if before.nodes.len() < BUSY_BOARD || self.page_quiet > 0 {
            return;
        }
        let new = drawn.nodes.iter().find(|n| n.kind == Kind::Topic && !before.nodes.iter().any(|o| o.id == n.id) && !self.topics_offered.contains(&n.id));
        if let Some(topic) = new {
            self.topics_offered.insert(topic.id.clone());
            self.topic_hint = Some((topic.id.clone(), topic.label.chars().take(24).collect()));
            self.topic_hint_age = 0;
        }
    }

    /// Give the offered topic its own board, and carry on there.
    fn split_topic(&mut self, cx: &mut Context<Self>) {
        let Some((id, label)) = self.topic_hint.take() else { return };
        if self.replay.is_some() || !self.live.split_topic(&id, &tr("Moved “{t}” to a new board", "把「{t}」移到了新的一张板").replace("{t}", &label)) {
            cx.notify();
            return;
        }
        self.page_hint = None;
        self.page_quiet = QUIET_AFTER_TURNING;
        let history: Vec<Utterance> = self.live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect();
        self.session().open(self.live.graph.clone(), history);
        self.ink.borrow_mut().open(&self.live.graph, &self.live.memory);
        self.show_page_image();
        cx.notify();
    }

    /// Follow the material being talked about. A line asking for a board, or agreeing to the one offered,
    /// turns to it and is not drawn; it stays in the transcript. Otherwise a board the line is clearly
    /// about is offered. True when the line was such a request.
    fn follow_pages(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if self.live.pages() < 2 || self.replay.is_some() {
            return false;
        }
        let pages = follow::pages(&self.live.boards());
        let current = self.live.page();
        let agreed = self.page_hint.as_ref().map(|h| h.0).filter(|_| {
            let t = text.trim().trim_end_matches(|c: char| c.is_ascii_punctuation() || "。！？、".contains(c)).to_lowercase();
            ["跳过去", "翻过去", "过去看看", "切过去", "go there", "go to it", "switch to it", "そっちへ", "そっちに", "そこへ"].iter().any(|w| t == *w || t.ends_with(w))
        });
        if let Some(board) = follow::asked(text, &pages, current).or(agreed) {
            self.page_quiet = QUIET_AFTER_TURNING;
            self.live.turned_to(board);
            self.go_to_page(board, cx);
            return true;
        }
        if self.page_quiet > 0 {
            self.page_quiet -= 1;
            return false;
        }
        let about = follow::suggested(text, &pages, current);
        // Turning by itself, the line is drawn on the page it is about.
        if let Some(board) = about.filter(|_| self.form.saved.turn_pages) {
            self.page_quiet = QUIET_AFTER_TURNING;
            self.go_to_page(board, cx);
            return false;
        }
        self.page_hint = about.map(|b| (b, pages[b].number, pages[b].title.chars().take(32).collect()));
        false
    }

    fn go_to_page(&mut self, page: usize, cx: &mut Context<Self>) {
        if self.replay.is_some() {
            self.notice = Some(tr("Can't switch boards during replay. Exit replay first.", "回放时不能切换板，请先退出回放。").into());
            cx.notify();
            return;
        }
        self.live.go_to_page(page);
        self.page_hint = None;
        self.topic_hint = None;
        let history: Vec<Utterance> = self.live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect();
        self.session().open(self.live.graph.clone(), history);
        self.ink.borrow_mut().open(&self.live.graph, &self.live.memory);
        self.show_page_image();
        cx.notify();
    }

    fn pump_import(&mut self, changed: &mut bool) {
        let ImportState::Reading { updates, progress, .. } = &mut self.import else { return };
        loop {
            match updates.try_recv() {
                Ok(ImportUpdate::Progress(text)) => {
                    *progress = text;
                    *changed = true;
                }
                Ok(ImportUpdate::Finished(result)) => {
                    self.import = match result {
                        Ok(preview) => ImportState::Ready(preview),
                        Err(error) => {
                            eprintln!("import failed: {error}");
                            ImportState::Failed(error)
                        }
                    };
                    *changed = true;
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.import = ImportState::Failed("the import did not finish".to_string());
                    *changed = true;
                    return;
                }
            }
        }
    }

    fn pick_import_file(&mut self, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else { return };
            let _ = this.update(cx, |this, cx| this.import_paths(&paths, cx));
        })
        .detach();
    }

    fn import_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        self.panel = Panel::Import;
        match paths {
            [path] if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) => self.start_import(ImportSource::File(path.clone()), cx),
            [path] if path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_TYPES.contains(&e.to_ascii_lowercase().as_str())) => self.import_image_file(path.clone(), cx),
            [_] => {
                self.import = ImportState::Failed(tr("You can import PDFs and images (PNG, JPEG, WebP and GIF).", "可以导入 PDF 和图片（PNG、JPEG、WebP、GIF）。").to_string());
                cx.notify();
            }
            _ => {
                self.import = ImportState::Failed(tr("Choose one file at a time. Nothing was imported.", "一次选一个文件，什么都没有导入。").to_string());
                cx.notify();
            }
        }
    }

    /// The local model and add-on that can read pictures, if both are here.
    fn vision(&self) -> Option<Vision> {
        let model = boardcast_models::current(&self.models)?;
        let add_on = boardcast_models::vision_for(&model.file_name()?.to_string_lossy())?;
        boardcast_models::installed(&add_on, &self.models).then(|| Vision { binaries: self.binaries.clone(), model, projector: self.models.join(&add_on.file) })
    }

    /// Reading an image takes a model on this device, whichever model draws the board.
    fn import_image_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(model) = boardcast_models::current(&self.models) else {
            self.import = ImportState::Failed(tr("Reading text in an image takes a local model, and images are not sent anywhere else. Download one in Settings first.", "读图里的字需要本地模型，图片不会发到别处。请先在设置里下载一个。").to_string());
            cx.notify();
            return;
        };
        let file = model.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(add_on) = boardcast_models::vision_for(&file) else {
            self.import = ImportState::Failed(tr("This local model can't read images. To read one, choose Qwen3.5 or MiniCPM-V in Settings.", "这个本地模型不能看图。要读图，请在设置里选择 Qwen3.5 或 MiniCPM-V。").to_string());
            cx.notify();
            return;
        };
        if boardcast_models::installed(&add_on, &self.models) {
            let projector = self.models.join(&add_on.file);
            self.start_import(ImportSource::Image { path, binaries: self.binaries.clone(), model, projector }, cx);
        } else {
            self.import = ImportState::NeedsVision { path, add_on };
            cx.notify();
        }
    }

    fn import_arxiv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.arxiv_input.read(cx).value().to_string();
        match boardcast_import::arxiv::parse(&input) {
            Some(id) => {
                self.arxiv_input.update(cx, |state, cx| state.set_value("", window, cx));
                self.start_import(ImportSource::Arxiv(id), cx);
            }
            None => {
                self.import = ImportState::Failed(tr("That's not an arXiv ID: paste an arxiv.org/abs/… link or an ID like 2401.12345", "没认出 arXiv 编号：贴 arxiv.org/abs/… 的链接，或者 2401.12345 这样的编号").to_string());
                cx.notify();
            }
        }
    }

    /// Reading goes on elsewhere, and only prepares a preview, so the meeting carries on meanwhile.
    fn start_import(&mut self, source: ImportSource, cx: &mut Context<Self>) {
        if matches!(self.import, ImportState::Reading { .. }) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let library = self.pdfium.clone();
        let english = tr("en", "zh") == "en";
        let vision = self.vision();
        let (tx, updates) = channel();
        std::thread::spawn(move || {
            let result = read_import(source, vision, &library, english, &stop, &tx);
            let _ = tx.send(ImportUpdate::Finished(result.map_err(|e| format!("{e:#}"))));
        });
        self.import = ImportState::Reading { progress: tr("Reading the selected file…", "正在读取所选文件…").to_string(), cancel, updates };
        cx.notify();
    }

    fn stop_import(&mut self, cx: &mut Context<Self>) {
        if let ImportState::Reading { cancel, .. } = &self.import {
            cancel.store(true, Ordering::Relaxed);
        }
        self.import = ImportState::Idle;
        cx.notify();
    }

    /// Put a read file in a meeting. Its pictures are kept first, so a failure leaves the preview ready.
    fn place_import(&mut self, fresh: bool, window: &mut Window, cx: &mut Context<Self>) {
        let spec = self.import_pages.read(cx).value().to_string();
        let Some(wanted) = page_ranges(&spec) else {
            self.notice = Some(tr("Write pages as numbers and ranges, like 1-5, 8.", "页码写成数字和范围，比如 1-5, 8。").into());
            cx.notify();
            return;
        };
        if let ImportState::Ready(preview) = &mut self.import {
            if !wanted.is_empty() {
                let asked = |p: &NewPage| wanted.iter().any(|(a, b)| (*a..=*b).contains(&page_number(&p.graph)));
                if !preview.pages.iter().any(asked) {
                    self.notice = Some(tr("None of those pages are in the file.", "文件里没有这些页。").into());
                    cx.notify();
                    return;
                }
                // The paper's references are kept on its first page, and stay with whichever page comes first now.
                let references = preview.pages.first().and_then(|p| p.graph.extra.get("references").cloned());
                preview.pages.retain(asked);
                if let (Some(first), Some(references)) = (preview.pages.first_mut(), references) {
                    first.graph.extra.insert("references".into(), references);
                }
            }
        }
        self.import_pages.update(cx, |state, cx| state.set_value("", window, cx));
        self.place_read_import(fresh, cx);
    }

    fn place_read_import(&mut self, fresh: bool, cx: &mut Context<Self>) {
        self.exit_replay(cx);
        if self.live.lines.iter().any(|l| l["ms"].is_null() && !l["manual"].as_bool().unwrap_or(false)) {
            self.notice = Some(tr("The model is still drawing. Import when it is done.", "模型还在画，画完再导入。").into());
            cx.notify();
            return;
        }
        let Some(store) = self.store.clone() else { return };
        if !matches!(self.import, ImportState::Ready(_)) || (fresh && !self.leave()) {
            cx.notify();
            return;
        }
        let ImportState::Ready(preview) = std::mem::replace(&mut self.import, ImportState::Idle) else { return };
        let keep = if fresh { Live::new() } else { std::mem::take(&mut self.live) };
        let id = keep.id().to_string();
        let mut stored = vec![];
        for page in &preview.pages {
            match store.put_image(&id, "image/jpeg", &page.jpeg) {
                Ok(reference) => stored.push(reference),
                Err(error) => {
                    self.notice = Some(format!("{}{error:#}", tr("The imported pages could not be saved. Nothing was added: ", "导入页保存失败，什么都没有加进去：")));
                    if !fresh {
                        self.live = keep;
                    }
                    self.import = ImportState::Ready(preview);
                    cx.notify();
                    return;
                }
            }
        }
        let mut live = keep;
        if fresh {
            if let Some(title) = &preview.title {
                live.set_title(title);
            }
        }
        let added = preview.pages.into_iter().map(|p| p.graph).zip(stored).collect();
        live.add_pages(added, &preview.note);
        self.show(live);
        self.live.changes += 1;
        self.panel = Panel::Transcript;
        cx.notify();
    }

    /// Put this board's imported page on screen. A page that cannot be read is said, and the rest shown.
    fn show_page_image(&mut self) {
        self.show_board_image(self.live.page());
    }

    /// The imported page's picture under a board, or none.
    fn show_board_image(&mut self, board: usize) {
        let reference = self.live.board_images().into_iter().nth(board).flatten();
        let read = reference.map(|r| match &self.store {
            Some(store) => store.page_image(self.live.id(), &r),
            None => boardcast_meetings::decode_data_url(&r),
        });
        let picture = read.and_then(|image| match image.map_err(|e| format!("{e:#}")).and_then(|i| page_render_image(&i.bytes)) {
            Ok(picture) => Some(picture),
            Err(error) => {
                self.notice = Some(format!("{}{error}", tr("The imported page could not be shown: ", "导入页显示失败：")));
                None
            }
        });
        let mut ink = self.ink.borrow_mut();
        let size = self.live.boards().get(board).and_then(Graph::underlay);
        let next = picture.zip(size).map(|(picture, (w, h))| (picture, w as f32, h as f32));
        if let Some((old, _, _)) = std::mem::replace(&mut ink.page, next) {
            ink.retired.push(old);
        }
    }

    /// Toggle a mark by hand. It is drawn at once, in the same pen as a mark from the model.
    fn mark(&mut self, id: &str, mark: &str, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let span = {
            let ink = self.ink.borrow();
            let unit = match ink.mark_unit {
                MarkUnit::Word => Some(Unit::Word),
                MarkUnit::Sentence => Some(Unit::Sentence),
                MarkUnit::Block => None,
            };
            let rows = self.live.graph.nodes.iter().find(|n| n.id == id).map_or(vec![], |n| n.rows());
            unit.zip(ink.selected_at).and_then(|(unit, (x, y))| span_at(&rows, x, y, mark, unit))
        };
        let graph = match span {
            Some(span) => self.session().toggle_span(id, span),
            None => self.session().toggle_mark(id, mark),
        };
        self.next_batch += 1;
        let mut ink = self.ink.borrow_mut();
        ink.update(&graph, self.next_batch);
        let memory = ink.board.snapshot();
        drop(ink);
        self.live.change_board(graph, memory);
        cx.notify();
    }

    /// Apply a change made by hand, as a mark is: at once, undoable and in the replay.
    fn edit_board(&mut self, ops: &[Op], cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let graph = self.session().edit(ops);
        self.next_batch += 1;
        let mut ink = self.ink.borrow_mut();
        ink.update(&graph, self.next_batch);
        let memory = ink.board.snapshot();
        drop(ink);
        self.live.change_board(graph, memory);
        cx.notify();
    }

    fn start_label(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(label) = self.live.graph.nodes.iter().find(|n| n.id == id).map(|n| n.label.clone()) else { return };
        self.label_input.update(cx, |state, cx| {
            state.set_value(label, window, cx);
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        self.ink.borrow_mut().editing = true;
        cx.notify();
    }

    /// Put the new label on the board, or leave the old one when `keep` is false or nothing changed.
    fn finish_label(&mut self, keep: bool, window: &mut Window, cx: &mut Context<Self>) {
        let id = {
            let mut ink = self.ink.borrow_mut();
            if !std::mem::take(&mut ink.editing) {
                return;
            }
            ink.selected.clone()
        };
        let label = self.label_input.read(cx).value().trim().to_string();
        if let Some(id) = id.filter(|_| keep && !label.is_empty()) {
            self.edit_board(&[Op::R(Ref::Id(id), label)], cx);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn remove(&mut self, id: &str, cx: &mut Context<Self>) {
        self.ink.borrow_mut().selected = None;
        self.edit_board(&[Op::D(Ref::Id(id.to_string()))], cx);
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let snap = if forward { self.live.redo() } else { self.live.undo() };
        if let Some(snap) = snap {
            self.session().set_graph(snap.graph.clone());
            self.next_batch += 1;
            let mut ink = self.ink.borrow_mut();
            ink.board.restore(&snap.memory);
            ink.update(&snap.graph, self.next_batch);
        }
        cx.notify();
    }

    fn tidy(&mut self, cx: &mut Context<Self>) {
        let mut ink = self.ink.borrow_mut();
        ink.tidy();
        let memory = ink.board.snapshot();
        drop(ink);
        self.live.change_board(self.live.graph.clone(), memory);
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" && self.ink.borrow().editing {
            self.finish_label(false, window, cx);
            return;
        }
        let keys = &event.keystroke;
        // Escape in a text field, such as a setting or the line being typed, leaves it: a side panel
        // closes back to the transcript, and the board has the keys again.
        let plain = !(keys.modifiers.control || keys.modifiers.platform || keys.modifiers.alt || keys.modifiers.shift);
        if keys.key == "escape" && plain && !self.focus.is_focused(window) {
            if self.panel != Panel::Transcript {
                self.panel = Panel::Transcript;
            }
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        }
        // Text fields keep their own undo.
        if !self.focus.is_focused(window) {
            return;
        }
        if let Some(at) = self.replay.as_ref().map(|r| r.player.at) {
            match keys.key.as_str() {
                "escape" => self.exit_replay(cx),
                "r" if keys.modifiers.shift && (keys.modifiers.control || keys.modifiers.platform) => self.exit_replay(cx),
                "space" => self.toggle_replay(cx),
                "left" => self.seek_replay(at.saturating_sub(1), cx),
                "right" => self.seek_replay(at + 1, cx),
                _ => {}
            }
            return;
        }
        if keys.modifiers.control || keys.modifiers.platform {
            match keys.key.as_str() {
                "r" if keys.modifiers.shift => self.start_replay(window, cx),
                "z" if keys.modifiers.shift => self.step(true, cx),
                "z" => self.step(false, cx),
                "y" => self.step(true, cx),
                "=" | "+" => self.zoom_by(ZOOM_STEP, cx),
                "-" => self.zoom_by(1.0 / ZOOM_STEP, cx),
                "0" => self.fit_board(cx),
                _ => {}
            }
            return;
        }
        if keys.modifiers.alt || keys.modifiers.shift {
            return;
        }
        match keys.key.as_str() {
            // Closes the nearest thing first: the selection, then a panel.
            "escape" if self.ink.borrow().circling => {
                self.ink.borrow_mut().clear_circles();
                cx.notify();
            }
            "escape" => {
                let deselected = self.ink.borrow_mut().selected.take().is_some();
                if !deselected && self.panel != Panel::Transcript {
                    self.panel = Panel::Transcript;
                }
                cx.notify();
            }
            "space" if self.editable() => {
                self.toggle_mic();
                cx.notify();
            }
            "/" if self.editable() => self.focus_input(window, cx),
            _ => {}
        }
    }

    /// Zoom around the middle of the board.
    fn zoom_by(&mut self, factor: f32, cx: &mut Context<Self>) {
        let mut ink = self.ink.borrow_mut();
        let middle = ink.canvas.center();
        ink.zoom_at(middle, factor);
        cx.notify();
    }

    fn fit_board(&mut self, cx: &mut Context<Self>) {
        self.ink.borrow_mut().show_latest();
        cx.notify();
    }

    /// Bring out the transcript and its input, ready to type a line.
    fn focus_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.panel = Panel::Transcript;
        self.transcript_open = true;
        self.input.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Take a meeting off the list; it can be brought back until the list is opened again.
    fn delete_meeting(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(store) = &self.store else { return };
        match store.delete(id) {
            Ok(()) => {
                let title = self.library.iter().find(|m| m.id == id).map(|m| m.title.clone()).unwrap_or_default();
                self.library.retain(|m| m.id != id);
                if self.pinned.iter().any(|p| p == id) {
                    let _ = store.set_pinned(id, false);
                    self.pinned.retain(|p| p != id);
                }
                self.deleted_meeting = Some((id.to_string(), title));
            }
            Err(e) => self.notice = Some(format!("{}{e:#}", tr("Could not delete the meeting: ", "删除会议失败："))),
        }
        cx.notify();
    }

    fn restore_meeting(&mut self, cx: &mut Context<Self>) {
        let (Some(store), Some((id, _))) = (&self.store, self.deleted_meeting.take()) else { return };
        match store.restore(&id).and_then(|()| store.list()) {
            Ok(list) => self.library = list,
            Err(e) => self.notice = Some(format!("{}{e:#}", tr("Could not bring the meeting back: ", "恢复会议失败："))),
        }
        cx.notify();
    }

    fn pin_meeting(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(store) = &self.store else { return };
        let pinned = !self.pinned.iter().any(|p| p == id);
        match store.set_pinned(id, pinned) {
            Ok(()) => self.pinned = store.pinned(),
            Err(e) => self.notice = Some(format!("{e:#}")),
        }
        cx.notify();
    }

    fn toggle_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.panel = if self.panel == panel { Panel::Transcript } else { panel };
        if self.panel == Panel::Settings {
            self.inputs = boardcast_audio::inputs();
        }
        if self.panel == Panel::Library {
            if let Some(store) = &self.store {
                match store.list() {
                    Ok(list) => {
                        self.library = list;
                        self.pinned = store.pinned();
                        self.deleted_meeting = None;
                    }
                    Err(e) => self.notice = Some(format!("{}{e:#}", tr("Could not list meetings: ", "读取会议列表失败："))),
                }
            }
        }
        cx.notify();
    }

    fn form_settings(&self, cx: &App) -> Settings {
        let value = |input: &Entity<InputState>| input.read(cx).value().trim().to_string();
        let key = value(&self.form.key);
        Settings {
            base_url: value(&self.form.base_url),
            model: value(&self.form.model),
            // An empty field keeps the saved key, which is never shown again.
            api_key: if key.is_empty() { self.form.saved.api_key.clone() } else { key },
            enabled: self.form.remote,
            diarize: self.form.saved.diarize,
            turn_pages: self.form.saved.turn_pages,
            glossary: self.form.saved.glossary.clone(),
            templates: self.form.saved.templates.clone(),
            system_speech: self.form.saved.system_speech,
            speech_language: self.form.saved.speech_language.clone(),
            interface_language: self.form.saved.interface_language.clone(),
            appearance: self.form.saved.appearance.clone(),
            input: self.form.saved.input.clone(),
            meeting_search: self.form.saved.meeting_search,
            messy: self.form.saved.messy,
        }
    }

    /// Listen to another input from now on, switching over at once if listening.
    fn choose_input(&mut self, id: String, cx: &mut Context<Self>) {
        self.form.saved.input = id;
        if let Err(e) = self.form.saved.save() {
            self.form.note = Some(format!("{e:#}"));
        }
        if self.pipeline.listening() {
            self.toggle_mic();
            self.toggle_mic();
        }
        cx.notify();
    }

    fn choose_appearance(&mut self, appearance: &str, cx: &mut Context<Self>) {
        self.form.saved.appearance = appearance.to_string();
        if let Err(e) = self.form.saved.save() {
            self.form.note = Some(format!("{e:#}"));
        }
        // Read before choosing: choosing light or dark settles it at once.
        let was_dark = theme::is_dark();
        theme::choose(appearance);
        self.restyle(was_dark, cx);
    }

    /// Put the light or dark palette into effect now. Board ink is baked in the palette it was drawn in,
    /// so a change of light or dark repaints it.
    fn restyle(&mut self, was_dark: bool, cx: &mut Context<Self>) {
        theme::apply(cx);
        if theme::is_dark() != was_dark {
            self.ink.borrow_mut().recolor();
        }
        cx.refresh_windows();
    }

    /// Write in a neater or messier hand from now on. What is on the board is written again in it at once,
    /// where it was, and the view stays where it is.
    fn choose_messy(&mut self, messy: f32, cx: &mut Context<Self>) {
        let messy = (messy.clamp(0.0, 1.0) * 10.0).round() / 10.0;
        if (self.form.saved.messy - messy).abs() < 0.001 {
            return;
        }
        self.form.saved.messy = messy;
        if let Err(e) = self.form.saved.save() {
            self.form.note = Some(format!("{e:#}"));
        }
        let mut ink = self.ink.borrow_mut();
        ink.glyphs.set_messy(messy);
        // A replay writes each step afresh, in whatever hand is chosen by then.
        if self.replay.is_none() {
            let (view, reading) = (ink.view, ink.reading);
            ink.open(&self.live.graph, &self.live.memory);
            (ink.view, ink.reading) = (view, reading);
        }
        drop(ink);
        cx.notify();
    }

    fn choose_language(&mut self, language: &str, cx: &mut Context<Self>) {
        self.form.saved.interface_language = language.to_string();
        crate::i18n::choose(language);
        if let Err(e) = self.form.saved.save() {
            self.form.note = Some(format!("{e:#}"));
        }
        cx.notify();
    }

    fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.form_settings(cx);
        if settings.enabled && !settings.is_complete() {
            self.form.note = Some(tr("Fill in the address, model and API key first.", "请先填写地址、模型和 API key。").into());
            cx.notify();
            return;
        }
        match settings.save() {
            Ok(()) => {
                self.form.saved = settings;
                self.form.key.update(cx, |state, cx| state.set_value("", window, cx));
                self.form.note = Some(tr("Saved.", "已保存。").into());
                self.apply_backend();
            }
            Err(e) => self.form.note = Some(format!("{}{e:#}", tr("Could not save: ", "保存失败："))),
        }
        cx.notify();
    }

    fn test_settings(&mut self, cx: &mut Context<Self>) {
        let settings = self.form_settings(cx);
        if !settings.is_complete() {
            self.form.note = Some(tr("Fill in the address, model and API key first.", "请先填写地址、模型和 API key。").into());
            cx.notify();
            return;
        }
        let provider = Provider::Remote { base_url: settings.base_url, key: settings.api_key, model: settings.model };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let hello = Utterance { speaker: None, text: "Hello".into(), pointing: vec![] };
            let reply = propose(&provider, &Graph::default(), &[], &hello, Guidance::default(), None);
            let _ = tx.send(reply.map(|r| r.ms).map_err(|e| format!("{e:#}")));
        });
        self.form.probing = Some(rx);
        self.form.note = Some(tr("Testing…", "正在测试…").into());
        cx.notify();
    }

    fn retry_line(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((speaker, text)) = self.live.retry_line(index) else { return };
        let (meeting, renames) = (self.session().meeting(), self.session().renames_seen());
        let _ = self.pipeline.said.send(Said { line: Utterance { speaker, text, pointing: vec![] }, ended: Instant::now(), asr_ms: None, meeting, renames });
        cx.notify();
    }

    /// What is circled goes with the next line, and the circles go away.
    fn take_pointing(&mut self) -> Vec<String> {
        let mut ink = self.ink.borrow_mut();
        let pointed = std::mem::take(&mut ink.pointed);
        ink.clear_circles();
        pointed
    }

    /// A speaker stands out while the pointer is on them, and on a touch screen, with no pointer, while
    /// they are chosen in the speakers panel.
    fn traced_speaker(&self) -> Option<&String> {
        self.speaker_hovered.as_ref().or(self.speaker_chosen.as_ref().filter(|_| self.panel == Panel::Speakers))
    }

    fn hover_speaker(&mut self, name: &str, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.speaker_hovered = Some(name.to_string());
        } else if self.speaker_hovered.as_deref() == Some(name) {
            self.speaker_hovered = None;
        }
        cx.notify();
    }

    /// Go to the line that put this node on the board.
    fn jump_to_source(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(node) = self.live.graph.nodes.iter().find(|n| n.id == id) else { return };
        let Some(who) = by_of_node(node) else { return };
        let mut names = vec![node.label.as_str()];
        names.extend(node.aliases.iter().map(String::as_str));
        let found = said_at(&self.live.lines, who, &names, self.live.page(), first_seen(&self.live.steps, id));
        let Some(line) = found else {
            self.notice = Some(tr("The line behind this isn't in the transcript.", "会议记录里找不到这条板书对应的话。").into());
            cx.notify();
            return;
        };
        self.panel = Panel::Transcript;
        self.transcript_open = true;
        self.jumped_line = Some((line, Instant::now()));
        cx.notify();
    }

    /// A voice says who it is. A sentence too short to tell goes to whoever spoke last.
    fn who_spoke(&mut self, voiceprint: Option<&[f32]>, seconds: f64) -> Option<String> {
        if let Some(print) = voiceprint.filter(|_| self.form.saved.diarize) {
            if let Some(name) = self.live.identify_voice(print, seconds) {
                self.speaker = Some(name);
            }
        }
        self.speaker.clone()
    }

    fn choose_speaker(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.speaker_name.update(cx, |state, cx| state.set_value(name.clone(), window, cx));
        self.speaker_chosen = Some(name);
        self.panel = Panel::Speakers;
        cx.notify();
    }

    /// Give one line to someone else, leaving the rest of its speaker's lines alone.
    fn move_line(&mut self, index: usize, to: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        self.live.move_line(index, &to);
        self.speaker_hovered = None;
        self.choose_speaker(to, window, cx);
    }

    fn rename_speaker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.speaker_name.read(cx).value().trim().to_string();
        let Some(from) = self.speaker_chosen.clone() else { return };
        if !self.editable() || to.is_empty() || to == from {
            return;
        }
        self.session().rename_speaker(&from, &to);
        self.live.rename_speaker(&from, &to);
        self.speaker_hovered = None;
        if self.speaker.as_deref() == Some(from.as_str()) {
            self.speaker = Some(to.clone());
        }
        self.choose_speaker(to, window, cx);
    }

    fn speakers_panel(&self, editable: bool, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        let names = self.live.speakers();
        let rows = names.iter().enumerate().map(|(i, name)| {
            let chosen = self.speaker_chosen.as_deref() == Some(name.as_str());
            let (lines, _, _) = self.live.speaker_impact(name);
            let (picked, hovered) = (name.clone(), name.clone());
            div()
                .id(("speaker-row", i))
                .on_hover(cx.listener(move |this, over: &bool, _, cx| this.hover_speaker(&hovered, *over, cx)))
                .flex()
                .items_center()
                .gap_2()
                .p_1()
                .rounded_md()
                .cursor_pointer()
                .when(chosen, |d| d.bg(rgb(pal().selected)))
                .child(speaker_badge(&names, name))
                .child(div().flex_1().min_w_0().text_sm().child(name.clone()))
                .when(self.speaker.as_deref() == Some(name.as_str()), |d| d.child(div().text_xs().text_color(rgb(pal().accent)).child(tr("next", "下一句"))))
                .child(div().text_xs().text_color(rgb(pal().faint)).child(format!("{lines} {}", tr("lines", "句"))))
                .on_click(cx.listener(move |this, _, window, cx| this.choose_speaker(picked.clone(), window, cx)))
        }).collect::<Vec<_>>();
        // A line picked in the transcript can go to someone else on its own, when it is still this speaker's.
        let one_line = self.speaker_line.filter(|_| editable).and_then(|i| {
            let line = self.live.lines.get(i)?;
            let from = line["speaker"].as_str().filter(|s| Some(*s) == self.speaker_chosen.as_deref())?.to_string();
            let said = line["text"].as_str().unwrap_or_default();
            let text: String = if said.chars().count() > 40 { said.chars().take(40).chain(['…']).collect() } else { said.to_string() };
            let someone_new = boardcast_board::speakers::next_name(names.iter().map(String::as_str));
            let others: Vec<(String, bool)> = names.iter().filter(|n| **n != from).map(|n| (n.clone(), false)).chain([(someone_new, true)]).collect();
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(pal().line))
                    .child(div().text_xs().text_color(rgb(pal().muted)).child(tr("Only this line was someone else:", "只有这一句是别人说的：")))
                    .child(div().text_sm().child(format!("“{text}”")))
                    .child(div().flex().flex_wrap().gap_1().children(others.into_iter().enumerate().map(|(k, (name, new))| {
                        let to = name.clone();
                        div()
                            .id(("move-line", k))
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_xs()
                            .border_1()
                            .border_color(rgb(pal().line))
                            .child(speaker_badge(&names, &name))
                            .child(if new { tr("Someone new ({name})", "新的人（{name}）").replace("{name}", &name) } else { name.clone() })
                            .on_click(cx.listener(move |this, _, window, cx| this.move_line(i, to.clone(), window, cx)))
                    })))
                    .child(div().text_xs().text_color(rgb(pal().faint)).child(tr("Its voice moves too, so later lines in that voice follow.", "声纹也一起挪过去，之后同样声音的句子会跟着走。"))),
            )
        });
        let editing = self.speaker_chosen.clone().filter(|name| editable && names.contains(name)).map(|from| {
            let to = self.speaker_name.read(cx).value().trim().to_string();
            let merging = to != from && names.contains(&to);
            let (lines, items, boards) = self.live.speaker_impact(&from);
            let affects = tr("Affects {lines} transcript lines and {items} board items on {boards} boards.", "会改动 {lines} 句记录，以及 {boards} 张板上的 {items} 项内容。")
                .replace("{lines}", &lines.to_string())
                .replace("{items}", &items.to_string())
                .replace("{boards}", &boards.to_string());
            let notice = (to != from && !to.is_empty()).then(|| {
                let text = if merging {
                    tr("“{to}” already exists, so “{from}” will become part of that speaker, voice included. This can't be undone.", "“{to}”已经存在，“{from}”会并入这个人，声纹也一起合并。这一步不能撤销。")
                } else {
                    tr("“{from}” becomes “{to}”, including lines still waiting for the board.", "“{from}”会改成“{to}”，还在排队上板的句子也一样。")
                };
                text.replace("{from}", &from).replace("{to}", &to)
            });
            let next = from.clone();
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_xs().text_color(rgb(pal().muted)).child(tr("New name", "新名字")))
                .child(Input::new(&self.speaker_name))
                .child(div().text_xs().text_color(rgb(pal().muted)).child(affects))
                .when_some(notice, |d, text| d.child(div().text_xs().text_color(rgb(if merging { pal().warning } else { pal().muted })).child(text)))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            button("rename-speaker", if merging { tr("Merge speakers", "合并说话人") } else { tr("Rename", "改名") })
                                .when(to.is_empty() || to == from, |b| b.opacity(0.4))
                                .on_click(cx.listener(|this, _, window, cx| this.rename_speaker(window, cx))),
                        )
                        .child(button("use-speaker", tr("Use for next line", "下一句用这个人")).on_click(cx.listener(move |this, _, _, cx| {
                            this.speaker = Some(next.clone());
                            cx.notify();
                        }))),
                )
        });
        div()
            .id("speakers-panel")
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .when(names.is_empty(), |d| d.child(div().text_sm().text_color(rgb(pal().faint)).child(tr("Nobody has spoken yet.", "还没有人说话。"))))
            .children(rows)
            .when_some(one_line, |d, one_line| d.child(one_line))
            .when_some(editing, |d, editing| d.child(editing))
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() || !self.editable() {
            return;
        }
        self.input.update(cx, |state, cx| state.set_value("", window, cx));
        let speaker = self.speaker.clone();
        self.live.add_line(speaker.as_deref(), &text, None);
        self.line_arrived(false);
        if self.follow_pages(&text, cx) {
            cx.notify();
            return;
        }
        let meeting = self.session().meeting();
        let renames = self.session().renames_seen();
        let pointing = self.take_pointing();
        let _ = self.pipeline.said.send(Said { line: Utterance { speaker, text, pointing }, ended: Instant::now(), asr_ms: None, meeting, renames });
        cx.notify();
    }

    /// On closing: save, and if that fails keep the window open once so the user can see why.
    fn closing(&mut self, cx: &mut Context<Self>) -> bool {
        if self.close_refused {
            return true;
        }
        self.count_listening();
        if let Err(error) = self.flush() {
            self.close_refused = true;
            self.notice = Some(format!("{}{error}", tr("Not saved, so the window stayed open. Close again to discard: ", "没有保存成功，窗口先不关。再关一次将丢弃未保存内容：")));
            cx.notify();
            return false;
        }
        true
    }
}

fn replay_slider(state: &Entity<SliderState>) -> impl IntoElement {
    Slider::new(state).accessibility_label(tr("Replay progress", "回放进度")).flex_1().min_w_0()
}

/// A summary as it reads on screen. To-dos can be ticked when `tick` is given, with a target big
/// enough for a finger.
fn summary_blocks(text: &str, group: &'static str, tick: Option<&Context<Workbench>>) -> Vec<gpui_kit::AnyElement> {
    let blocks = summary::blocks(text);
    // A section with to-dos says how many are done beside its name.
    let progress = |from: usize| {
        let todos: Vec<bool> = blocks[from + 1..].iter().take_while(|b| !matches!(b, Block::Heading(_))).filter_map(|b| if let Block::Todo { done, .. } = b { Some(*done) } else { None }).collect();
        (!todos.is_empty()).then(|| format!("{}/{}", todos.iter().filter(|d| **d).count(), todos.len()))
    };
    blocks
        .iter()
        .enumerate()
        .map(|(i, block)| match block {
            Block::Heading(text) => div()
                .flex()
                .items_baseline()
                .justify_between()
                .when(i > 0, |d| d.mt_4())
                .pb_1()
                .mb_1()
                .border_b_1()
                .border_color(rgb(pal().line))
                .child(div().text_sm().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(text.clone()))
                .when_some(progress(i), |d, done| d.child(div().font_family(theme::MONO).text_xs().text_color(rgb(pal().faint)).child(done)))
                .into_any_element(),
            Block::Paragraph(text) => div().py(px(3.)).text_sm().text_color(rgb(pal().muted)).child(text.clone()).into_any_element(),
            Block::Bullet(text) => div()
                .flex()
                .items_start()
                .gap_2()
                .py(px(3.))
                .text_sm()
                // A small square, as the board's own marks are drawn from blocks.
                .child(div().flex_shrink_0().mt(px(8.)).size(px(4.)).bg(rgb(pal().faint)))
                .child(div().flex_1().min_w_0().child(text.clone()))
                .into_any_element(),
            Block::Todo { done, text, line } => {
                let (done, line) = (*done, *line);
                let tick_box = div()
                    .flex_shrink_0()
                    .mt(px(3.))
                    .size(px(14.))
                    .border_1()
                    .border_color(rgb(pal().muted))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .text_color(rgb(pal().on_strong))
                    .when(done, |d| d.bg(rgb(pal().strong)).border_color(rgb(pal().strong)).child("✓"));
                let row = div()
                    .id((group, i))
                    .flex()
                    .items_start()
                    .gap_2()
                    .py(px(3.))
                    .text_sm()
                    .child(tick_box)
                    .child(div().flex_1().min_w_0().when(done, |d| d.text_color(rgb(pal().faint)).line_through()).child(text.clone()));
                match tick {
                    Some(cx) => row.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.tick_todo(line, cx))).into_any_element(),
                    None => row.into_any_element(),
                }
            }
        })
        .collect()
}

impl Workbench {
    fn replay_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<gpui_kit::Div> {
        let replay = self.replay.as_ref()?;
        let player = replay.player.clone();
        // Steps move on their own, and the slider has to follow.
        if replay.slider.read(cx).value().end() != player.at as f32 {
            let value = player.at as f32;
            replay.slider.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        let steps = &replay.steps;
        let time = format!("{} / {}", clock(player.elapsed_ms(steps)), clock(Player::duration_ms(steps)));
        let can_fork = player.at > 0 && self.live.read_only.is_none();
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .h(px(48.))
                .flex_shrink_0()
                .child(button("replay-toggle", if player.playing { tr("Pause", "暂停") } else { tr("Play", "播放") }).on_click(cx.listener(|this, _, _, cx| this.toggle_replay(cx))))
                .child(replay_slider(&replay.slider))
                .child(div().text_xs().whitespace_nowrap().text_color(rgb(pal().muted)).child(time))
                .child(button("replay-speed", format!("{}×", player.speed)).on_click(cx.listener(|this, _, _, cx| {
                    if let Some(replay) = &mut this.replay {
                        replay.player.next_speed();
                    }
                    this.wait_for_next_step();
                    cx.notify();
                })))
                .when(self.live.read_only.is_none(), |d| {
                    d.child(button("replay-fork", tr("Fork a board", "另起一板")).whitespace_nowrap().disabled(!can_fork).on_click(cx.listener(|this, _, _, cx| this.fork_replay(cx))))
                }),
        )
    }

    fn import_panel(&self, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        let hint = |text: &'static str| div().text_xs().text_color(rgb(pal().muted)).child(text);
        let panel = div().id("import-panel").flex_1().overflow_y_scroll().flex().flex_col().gap_2();
        let can_add_here = self.live.read_only.is_none();
        match &self.import {
            ImportState::NeedsVision { add_on, path } => {
                let (fetch, retry) = (add_on.clone(), path.clone());
                let downloading = self.download.as_ref().filter(|d| d.id == add_on.id);
                let progress = downloading.map(|d| match &d.progress {
                    Some(p) if p.verifying => tr("Checking what was already downloaded…", "正在检查已下载的部分…").to_string(),
                    Some(p) => format!("{} / {}", gigabytes(p.done), gigabytes(p.total)),
                    None => tr("Starting…", "正在开始…").to_string(),
                });
                let ready = boardcast_models::installed(add_on, &self.models);
                panel
                    .child(div().text_sm().child(tr("To read the text in an image, the local model needs its vision add-on. Images stay on this device.", "要读图里的字，本地模型需要看图组件。图片只在本机处理。")))
                    .child(hint(if downloading.is_some() { add_on.label } else { tr("One download, and then images just work.", "下载一次，之后都能用。") }))
                    .when_some(progress, |d, text| d.child(div().text_xs().child(text)))
                    .when(ready, |d| d.child(button("read-image", tr("Read the image", "读取图片")).on_click(cx.listener(move |this, _, _, cx| this.import_image_file(retry.clone(), cx)))))
                    .when(!ready && downloading.is_none() && self.download.is_none(), |d| {
                        d.child(button("get-vision", format!("{} · {}", tr("Download", "下载"), gigabytes(add_on.size))).on_click(cx.listener(move |this, _, _, cx| this.start_download(fetch.clone(), cx))))
                    })
                    .child(button("import-cancel", tr("Cancel", "取消")).on_click(cx.listener(|this, _, _, cx| this.stop_import(cx))))
            }
            ImportState::Reading { progress, .. } => panel
                .child(div().text_sm().child(progress.clone()))
                .child(hint(tr("You can carry on with the meeting while this reads.", "读取的时候，会议可以继续。")))
                .child(button("stop-import", tr("Stop", "停止")).on_click(cx.listener(|this, _, _, cx| this.stop_import(cx)))),
            ImportState::Ready(preview) => panel
                .child(div().text_sm().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(format!("{} · {} {}", preview.name, preview.pages.len(), tr("boards", "张板"))))
                .child(div().text_xs().text_color(rgb(pal().muted)).child(preview.ready.clone()))
                .children(preview.details.iter().map(|d| div().text_xs().text_color(rgb(pal().warning)).child(d.clone())))
                .when(preview.pages.len() > 1, |d| {
                    d.child(div().mt_1().text_xs().text_color(rgb(pal().muted)).child(tr("Pages to import", "要导入的页"))).child(Input::new(&self.import_pages).small())
                })
                .child(button("import-new", tr("New meeting", "新开一场会")).on_click(cx.listener(|this, _, window, cx| this.place_import(true, window, cx))))
                .when(can_add_here, |d| d.child(button("import-here", tr("Add to this meeting", "加到这场会")).on_click(cx.listener(|this, _, window, cx| this.place_import(false, window, cx)))))
                .child(button("import-cancel", tr("Cancel", "取消")).on_click(cx.listener(|this, _, window, cx| {
                    this.import_pages.update(cx, |state, cx| state.set_value("", window, cx));
                    this.stop_import(cx);
                }))),
            state => panel
                .when_some(if let ImportState::Failed(error) = state { Some(error.clone()) } else { None }, |d, error| d.child(div().text_sm().text_color(rgb(pal().danger)).child(error)))
                .child(hint(tr("A PDF page or an image becomes a board, with its text in place to circle, link and note on during the meeting.", "PDF 的每一页或一张图片变成一张板，文字留在原位，开会时可以圈画、连线、写批注。")))
                // Where a file goes: the button, or a drop anywhere in the window.
                .child(
                    div()
                        .mt_2()
                        .py_6()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .border_1()
                        .border_dashed()
                        .border_color(rgb(pal().faint))
                        .rounded(px(3.))
                        .child(Button::new("pick-import").small().primary().label(tr("Choose a file…", "选择文件…")).on_click(cx.listener(|this, _, _, cx| this.pick_import_file(cx))))
                        .child(hint(tr("or drop a PDF or an image anywhere in the window", "或者把 PDF、图片拖进窗口"))),
                )
                .child(div().mt_3().text_xs().text_color(rgb(pal().muted)).child(tr("An arXiv paper, by link or ID", "arXiv 论文的链接或编号")))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&self.arxiv_input).small()))
                        .child(button("import-arxiv", tr("Download and read", "下载并读取")).on_click(cx.listener(|this, _, window, cx| this.import_arxiv(window, cx)))),
                ),
        }
    }

    fn export_panel(&self, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        let drawn = !self.live.graph.nodes.is_empty() || self.live.graph.underlay().is_some();
        let content = self.live.has_content();
        let formats = [
            (ExportKind::Markdown, "Markdown", tr("Key points, a Mermaid flow chart and the transcript.", "要点、Mermaid 流程图和会议记录。"), content),
            (ExportKind::Png, tr("PNG image", "PNG 图片"), tr("This board, as it looks on screen.", "当前这张板，和画面上一样。"), drawn),
            (ExportKind::Svg, tr("SVG vector", "SVG 矢量图"), tr("This board. Stays sharp at any size.", "当前这张板，放大不糊。"), drawn),
            (ExportKind::AnimatedSvg, tr("Animated SVG", "动态 SVG"), tr("This board, written out stroke by stroke in the order of the meeting.", "当前这张板，按会议顺序逐笔写出。"), drawn),
            (ExportKind::Gif, "GIF", tr("This board, written out stroke by stroke, for chats and documents that don't play SVG.", "当前这张板，逐笔写出，可以直接贴进群聊和文档。"), drawn),
            (ExportKind::ReplayPage, tr("Replay page", "回放网页"), tr("One .html file for someone who missed the meeting. Nothing to install.", "一个 .html 文件，发给没参会的人，双击就能看。"), content),
            (ExportKind::Pdf, tr("Whole meeting PDF", "整场会 PDF"), tr("A page per board, then the summary and transcript, with selectable text.", "每张板一页，后面是摘要和会议记录，文字可以选中复制。"), content),
        ];
        let busy = self.exporting;
        let rows = formats.into_iter().enumerate().map(|(i, (kind, name, hint, available))| {
            let enabled = available && busy.is_none();
            // A row of the manual's table, as on the website: the format, what it holds, and its file.
            div()
                .id(("export", i))
                .flex()
                .items_start()
                .justify_between()
                .gap_3()
                .py_2()
                .border_b_1()
                .border_color(rgb(pal().line))
                .when(!enabled, |d| d.opacity(0.4))
                .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(rgb(pal().selected))).on_click(cx.listener(move |this, _, _, cx| this.start_export(kind, cx))))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().child(if busy == Some(kind) { format!("{name} · {}", tr("Exporting…", "正在导出…")) } else { name.to_string() }))
                        .child(div().text_xs().text_color(rgb(pal().muted)).child(hint)),
                )
                .child(div().pt(px(2.)).font_family(theme::MONO).text_xs().text_color(rgb(pal().faint)).whitespace_nowrap().child(format!(".{} ↓", kind.extension())))
        });
        let result = self.exported.as_ref().map(|result| match result {
            Exported::Saved { path, text_left_out } => {
                let shown = path.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().child(format!("{} {}", tr("Saved to", "已保存到"), path.display())))
                    .when(*text_left_out, |d| {
                        d.child(div().text_xs().text_color(rgb(pal().warning)).child(tr(
                            "No usable font was found, so the summary and transcript are not in this PDF. Markdown has them.",
                            "没有找到可用的字体，这份 PDF 里没有摘要和会议记录。可以导出 Markdown 获取这些内容。",
                        )))
                    })
                    .child(button("reveal-export", tr("Show in folder", "在文件夹中显示")).on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&shown))))
            }
            Exported::Failed(error) => div().text_xs().text_color(rgb(pal().danger)).child(format!("{}{error}", tr("Could not export: ", "导出失败："))),
        });
        div()
            .id("export-panel")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2()
            .when(self.live.read_only == Some(ViewOnly::ImportedContent), |d| {
                d.child(div().text_xs().text_color(rgb(pal().warning)).child(tr("Free-edit content is not included.", "自由编辑内容不会导出。")))
            })
            .children(rows)
            .when_some(result, |d, result| d.child(result))
    }

    fn summary_panel(&mut self, editable: bool, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        // Reading the whole meeting is only worth doing once per change.
        let (id, changes) = (self.live.id().to_string(), self.live.changes);
        if !self.summary.current.as_ref().is_some_and(|(i, c, _)| *i == id && *c == changes) {
            self.summary.current = Some((id, changes, fingerprint_of(&self.live)));
        }
        let current = self.summary.current.as_ref().map_or("", |(_, _, value)| value.as_str());
        let text = self.live.summary().to_string();
        // A view-only meeting cannot change, and its board may not have been read in full.
        let stale = editable && !text.is_empty() && self.live.summary_source().is_some_and(|source| source != current);
        let draft_stale = self.summary.draft.as_ref().is_some_and(|(_, source)| source != current);
        let busy = self.summary.job.as_ref().map(|job| match job.step {
            Some((done, total)) if total > 1 && done + 1 < total => format!("{} {}/{}…", tr("Writing part", "分段整理"), done + 1, total - 1),
            Some((_, total)) if total > 1 => tr("Putting it together…", "合成摘要…").to_string(),
            _ => tr("Writing…", "整理中…").to_string(),
        });
        let can_make = editable && self.summary.draft.is_none() && (self.live.has_content() || !text.is_empty());
        let destination = match self.session().provider() {
            Ok(Provider::Remote { base_url, model, .. }) => {
                let host = base_url.split("//").last().unwrap_or(&base_url).split('/').next().unwrap_or_default().to_string();
                Some(format!("{} {model}{} {host}", tr("Uses the cloud model", "用云端的"), tr("; the whole transcript is sent to", "生成：整场会议记录会发到")))
            }
            _ => None,
        };
        let has_text = !text.is_empty();
        let editing = self.summary.editing;
        // One line at the top on where the summary stands, with what can be done to it beside.
        let (state, state_color): (Option<String>, u32) = match &busy {
            Some(status) => (Some(status.clone()), pal().recording),
            None if stale => (Some(tr("The meeting has moved on", "会议有了新内容").to_string()), pal().warning),
            None => (self.summary.note.clone(), pal().muted),
        };
        let bar = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .h(px(40.))
            .flex_none()
            .border_b_1()
            .border_color(rgb(pal().line))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .text_xs()
                    .text_color(rgb(state_color))
                    .when_some(state, |d, state| {
                        d.when(busy.is_some() || stale, |d| d.child(div().size(px(6.)).flex_none().rounded_full().bg(rgb(state_color)))).child(div().overflow_hidden().whitespace_nowrap().child(state))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_none()
                    .when(editable && busy.is_some(), |d| d.child(button("stop-summary", tr("Stop", "停止")).on_click(cx.listener(|this, _, _, cx| this.stop_summary(cx)))))
                    .when(editable && busy.is_none() && has_text && !editing, |d| {
                        d.child(
                            Button::new("make-summary")
                                .small()
                                .map(|b| if stale { b.primary() } else { b.outline() })
                                .label(tr("Write again", "重新生成"))
                                .disabled(!can_make)
                                .when(can_make, |b| b.on_click(cx.listener(|this, _, _, cx| this.make_summary(cx)))),
                        )
                    })
                    .when(has_text && !editing, |d| {
                        let (markdown, plain) = (text.clone(), summary::plain_text(&text));
                        d.child(icon_button("copy-plain", "copy", tr("Copy as plain text", "复制为纯文本")).on_click(cx.listener(move |this, _, _, cx| this.copy_summary(plain.clone(), tr("Copied as plain text", "已复制纯文本"), cx))))
                            .child(
                                Button::new("copy-markdown")
                                    .small()
                                    .ghost()
                                    .label("MD")
                                    .tooltip(tr("Copy the Markdown source", "复制 Markdown 原文"))
                                    .on_click(cx.listener(move |this, _, _, cx| this.copy_summary(markdown.clone(), tr("Copied the Markdown source", "已复制 Markdown 原文"), cx))),
                            )
                    })
                    .when(editable && has_text && !editing, |d| {
                        d.child(icon_button("edit-summary", "pencil", tr("Edit", "修改")).on_click(cx.listener(|this, _, window, cx| this.edit_summary(window, cx))))
                            .child(icon_button("clear-summary", "trash", tr("Delete", "删除")).on_click(cx.listener(|this, _, window, cx| this.clear_summary(window, cx))))
                    })
                    .when(editing, |d| d.child(Button::new("edit-summary").small().primary().label(tr("Done", "完成")).on_click(cx.listener(|this, _, window, cx| this.edit_summary(window, cx))))),
            );
        let tick = editable.then_some(&*cx);
        let reading = (!editing).then(|| summary_blocks(&text, "todo", tick)).unwrap_or_default();
        let proposed = self.summary.draft.as_ref().map(|(draft, _)| summary_blocks(draft, "draft-todo", None));
        let empty = !has_text && !editing && busy.is_none() && self.summary.draft.is_none() && self.summary.cleared.is_none();
        let body = div()
            .id("summary-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .when(self.summary.cleared.is_some(), |d| {
                d.child(div().flex().items_center().justify_between().gap_2().mb_3().px_3().py_2().bg(rgb(pal().selected)).child(div().text_xs().child(tr("Summary deleted.", "摘要已删除。"))).child(button("undo-clear", tr("Undo", "撤销")).on_click(cx.listener(|this, _, _, cx| this.undo_clear(cx)))))
            })
            .when(busy.is_some() && has_text, |d| d.child(div().mb_3().text_xs().text_color(rgb(pal().muted)).child(tr("Your current text stays here and can be edited while the new one is written.", "正在生成新摘要，当前内容仍可查看和编辑。"))))
            // Nothing yet: what it will hold, and the one thing to do.
            .when(empty, |d| {
                d.child(
                    div()
                        .pt_10()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .child(icon("sparkles").size_6().text_color(rgb(pal().faint)))
                        .child(div().text_sm().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(tr("No summary yet", "还没有摘要")))
                        .child(div().max_w(px(240.)).text_xs().text_center().text_color(rgb(pal().muted)).child(tr("A draft from what was said: conclusions, decisions, to-dos and open questions. You can edit it.", "根据原话整理一份草稿：结论、决定、待办、待讨论，之后可以修改。")))
                        .when(editable, |d| {
                            d.child(
                                Button::new("make-summary")
                                    .small()
                                    .primary()
                                    .label(tr("Write summary", "生成摘要"))
                                    .disabled(!can_make)
                                    .when(can_make, |b| b.on_click(cx.listener(|this, _, _, cx| this.make_summary(cx)))),
                            )
                        })
                        .when_some(destination.clone().filter(|_| editable), |d, text| d.child(div().max_w(px(260.)).text_xs().text_center().text_color(rgb(pal().faint)).child(text))),
                )
            })
            .when(editing, |d| d.child(div().flex_1().min_h(px(320.)).border_1().border_color(rgb(pal().line)).p_2().text_sm().child(Textarea::new(&self.summary.editor))))
            .children(reading)
            .when_some(proposed, |d, proposed| {
                d.child(
                    div()
                        .mt_5()
                        .border_1()
                        .border_color(rgb(pal().strong))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .bg(rgb(pal().strong))
                                .text_color(rgb(pal().on_strong))
                                .child(div().text_xs().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(tr("New draft", "新草稿")))
                                .when(draft_stale, |d| d.child(div().text_xs().opacity(0.7).child(tr("the meeting has moved on since", "之后会议又有了新内容")))),
                        )
                        .child(div().p_3().flex().flex_col().children(proposed))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .px_3()
                                .pb_3()
                                .child(Button::new("use-draft").small().primary().label(tr("Replace current", "替换当前摘要")).on_click(cx.listener(|this, _, _, cx| this.use_draft(cx))))
                                .child(button("keep-current", tr("Keep current", "保留当前")).on_click(cx.listener(|this, _, _, cx| {
                                    this.summary.draft = None;
                                    cx.notify();
                                }))),
                        ),
                )
            });
        div().id("summary-panel").flex_1().min_h_0().flex().flex_col().child(bar).child(body)
    }
}

/// A speaker's letter in their color, which follows the order people first spoke in.
fn speaker_badge(speakers: &[String], name: &str) -> Tag {
    const COLORS: [(u32, u32); 6] = [(0xfc_d7_ea, 0xa6_1e_4d), (0xd6_e6_ff, 0x1c_4f_9c), (0xd4_f5_e7, 0x1f_7a_45), (0xfd_e8_c8, 0x9f_4f_0b), (0xe5_dc_ff, 0x5b_3c_c4), (0xe2_e8_f0, 0x47_55_69)];
    let (background, ink) = COLORS[speakers.iter().position(|s| s == name).unwrap_or(0) % COLORS.len()];
    Tag::custom(rgb(background).into(), rgb(ink).into(), rgb(background).into()).xsmall().rounded(px(10.)).child(initial(name))
}

/// A speaker's badge on a transcript line: in outline while the line waits for the board, filled once it is on it.
fn line_badge(speakers: &[String], name: &str, drawn: bool) -> Tag {
    if drawn {
        return speaker_badge(speakers, name);
    }
    const INKS: [u32; 6] = [0xa6_1e_4d, 0x1c_4f_9c, 0x1f_7a_45, 0x9f_4f_0b, 0x5b_3c_c4, 0x47_55_69];
    let ink = INKS[speakers.iter().position(|s| s == name).unwrap_or(0) % INKS.len()];
    Tag::custom(gpui_kit::transparent_black(), rgb(ink).into(), rgb(ink).into()).xsmall().rounded(px(10.)).child(initial(name))
}

/// The highlighter that goes with a speaker's badge: the same hue, strong enough to show under ink.
fn speaker_marker(speakers: &[String], name: &str) -> u32 {
    const MARKERS: [u32; 6] = [0xf7_83_ac, 0x74_c0_fc, 0x69_db_7c, 0xff_a9_4d, 0xb1_97_fc, 0xad_b5_bd];
    MARKERS[speakers.iter().position(|s| s == name).unwrap_or(0) % MARKERS.len()]
}

fn button(id: impl Into<gpui_kit::ElementId>, label: impl Into<gpui_kit::SharedString>) -> Button {
    Button::new(id).small().outline().label(label)
}

/// An icon that does something, named in a tooltip.
fn icon_button(id: &'static str, name: &'static str, tooltip: &'static str) -> Button {
    Button::new(id).small().ghost().icon(icon(name)).tooltip(tooltip)
}

fn case_or_measurement(turns: &[Utterance], wav: &Option<PathBuf>, demo: &Option<String>, synthetic: Option<usize>) -> bool {
    !turns.is_empty() || wav.is_some() || demo.is_some() || synthetic.is_some()
}

fn local_time(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map_or(String::new(), |t| t.format("%Y-%m-%d %H:%M").to_string())
}

/// Meetings that keep to a shape, with the topics they start from.
fn templates() -> Vec<(&'static str, Vec<String>)> {
    let topics = |list: [(&'static str, &'static str); 3]| list.iter().map(|(en, zh)| tr(en, zh).to_string()).collect::<Vec<_>>();
    vec![
        (tr("Weekly meeting", "周会"), topics([("Last week", "上周进展"), ("This week", "本周计划"), ("Risks", "风险")])),
        (tr("Retrospective", "复盘"), topics([("Went well", "做得好的"), ("To improve", "要改进的"), ("Actions", "行动项")])),
        (tr("Stand-up", "站会"), topics([("Yesterday", "昨天"), ("Today", "今天"), ("Blocked", "阻塞")])),
        (tr("Design review", "方案评审"), [("Background", "背景"), ("Options", "方案"), ("Risks", "风险"), ("Decision", "结论")].iter().map(|(en, zh)| tr(en, zh).to_string()).collect()),
    ]
}

/// A recorded sample replayed from its stored model output, with no model involved.
fn demo_board(root: &std::path::Path, name: &str) -> Result<Graph> {
    #[derive(Deserialize)]
    struct Demo {
        lines: Vec<DemoLine>,
    }
    #[derive(Deserialize)]
    struct DemoLine {
        ops: Option<String>,
    }
    let demo: Demo = serde_json::from_slice(&std::fs::read(root.join(format!("assets/demos/{name}.json")))?)?;
    let mut g = Graph::default();
    for line in demo.lines {
        if let Some(ops) = line.ops {
            g.apply(&parse_ops(&ops));
        }
    }
    Ok(g)
}

/// A big board from plain operations, for measuring size rather than the model.
fn synthetic_board(chars: usize) -> Graph {
    const NAMES: [&str; 8] = ["灰度发布", "回滚演练", "负载测试", "Release notes", "支付回调", "消息队列", "无障碍审计", "数据库缓存"];
    let mut ops = String::new();
    let mut written = 0;
    let mut i = 0;
    while written < chars {
        let topic = format!("话题{i}");
        ops += &format!("T 「{topic}」\n");
        written += topic.chars().count();
        for j in 0..6 {
            let item = format!("{}{i}-{j}", NAMES[(i + j) % NAMES.len()]);
            written += item.chars().count();
            ops += &format!("L 「{topic}」 「{item}」\n");
            match (i + j) % 7 {
                0 => ops += &format!("M 「{item}」 划掉\n"),
                1 => ops += &format!("M 「{item}」 圈出\n"),
                2 => ops += &format!("M 「{item}」 下划线\n"),
                3 => ops += &format!("M 「{item}」 星标\n"),
                4 => ops += &format!("M 「{item}」 高亮\n"),
                _ => {}
            }
        }
        if i % 4 == 0 && i < 40 {
            ops += &format!("E 「服务{i}」 -> 「服务{}」 : 「调用」\n", i + 4);
            ops += &format!("E 「服务{i}」 -> 「存储{i}」\n");
            written += 8;
        }
        i += 1;
    }
    let mut g = Graph::default();
    g.apply(&parse_ops(&ops));
    g
}

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4096 / 1_048_576)
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Only animate while there is ink to write, the view is moving or a line is being typed; an idle board
        // costs no frames.
        {
            let ink = self.ink.borrow();
            if !ink.queue.is_empty() || !ink.sliding.is_empty() || ink.zoom_test.is_some() || ink.gliding() || self.typing.as_ref().is_some_and(|t| t.animate) {
                window.request_animation_frame();
            }
        }
        let (reading, zoom) = {
            let ink = self.ink.borrow();
            (ink.reading, ink.shown().k)
        };
        let ink = self.ink.clone();
        let wheel = self.ink.clone();
        let press = self.ink.clone();
        let moved = self.ink.clone();
        let release = self.ink.clone();
        let listening = self.pipeline.listening();
        let mut board = div()
            .id("board")
            .absolute()
            .size_full()
            .overflow_hidden()
            .bg(rgb(pal().paper))
            .on_scroll(cx.listener(move |_, e: &ScrollEvent, window, cx| {
                let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
                // Same response as the current app's wheel: exponential in the scrolled distance.
                wheel.borrow_mut().zoom_at(e.position, (dy * 0.0015).exp());
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                window.focus(&this.focus, cx);
                let mut b = press.borrow_mut();
                if b.circling {
                    let start = b.on_board(e.position);
                    b.drawing = Some(vec![start]);
                    return;
                }
                let view = b.shown();
                b.drag = Some((e.position, view));
            }))
            .on_mouse_move(cx.listener(move |_, e: &MouseMoveEvent, _, cx| {
                let mut b = moved.borrow_mut();
                if b.drawing.is_some() && e.pressed_button == Some(MouseButton::Left) {
                    let at = b.on_board(e.position);
                    b.drawing.as_mut().expect("drawing").push(at);
                    cx.notify();
                    return;
                }
                if let Some((from, view)) = b.drag.filter(|_| e.pressed_button == Some(MouseButton::Left)) {
                    let d = e.position - from;
                    b.view = Some(View { x: view.x - f32::from(d.x) / view.k, y: view.y - f32::from(d.y) / view.k, k: view.k });
                    b.reading = true;
                    b.glide = None;
                    b.zoomed_at = Instant::now();
                    b.touched = Instant::now();
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(move |_, e: &MouseUpEvent, _, cx| {
                let mut b = release.borrow_mut();
                if b.drawing.is_some() {
                    b.close_loop();
                    cx.notify();
                    return;
                }
                if let Some((from, _)) = b.drag.take() {
                    let d = e.position - from;
                    if f32::from(d.x).hypot(f32::from(d.y)) < CLICK_SLOP {
                        b.click(e.position);
                        cx.notify();
                    }
                }
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, (), window: &mut Window, _: &mut App| {
                        let mut b = ink.borrow_mut();
                        b.canvas = bounds;
                        b.dpr = window.scale_factor();
                        let started = Instant::now();
                        let uploads_before = b.base.uploads() + b.detail.as_ref().map_or(0, |(t, _)| t.uploads());
                        // Before the first stroke: a big board must not start at full resolution.
                        if b.finished.is_empty() {
                            let level = raster_level(b.fit().k, b.dpr);
                            if b.base.level != level {
                                b.base = Tier::new(level);
                            }
                        }
                        b.sweep();
                        b.advance();
                        let v = b.shown();
                        let view = b.visible();
                        let origin = bounds.origin + point(px(-v.x * v.k), px(-v.y * v.k));
                        for image in std::mem::take(&mut b.retired) {
                            let _ = window.drop_image(image);
                        }
                        window.with_content_mask(Some(gpui_kit::ContentMask { bounds }), |window| {
                            let state = &mut *b;
                            let trace = state.trace.clone().map(|(who, marker)| {
                                let (shown, marked) = state.traced_areas(&who);
                                (marker, shown, marked)
                            });
                            // The detail tier replaces the base rather than overlaying it, or
                            // the two copies of each stroke would thicken and blur together.
                            let tier = match &mut state.detail {
                                Some((tier, cover)) if inside(view, *cover) => tier,
                                _ => &mut state.base,
                            };
                            // Faint dots under all the ink: enough to read as a sheet, too little to compete with it.
                            let dot: Hsla = rgb(pal().line).into();
                            let mut gy = bounds.origin.y + px(GRID);
                            while gy < bounds.origin.y + bounds.size.height {
                                let mut gx = bounds.origin.x + px(GRID);
                                while gx < bounds.origin.x + bounds.size.width {
                                    window.paint_quad(gpui_kit::fill(Bounds::new(point(gx - px(1.), gy - px(1.)), size(px(2.), px(2.))), dot));
                                    gx += px(GRID);
                                }
                                gy += px(GRID);
                            }
                            // Under everything, from the moment a replay reaches the page.
                            if let Some((image, w, h)) = state.page.clone().filter(|_| state.graph.as_ref().is_some_and(|g| g.underlay().is_some())) {
                                let area = Bounds::new(origin, size(px(w * v.k), px(h * v.k)));
                                let _ = window.paint_image(area, area, gpui_kit::Corners::default(), image, 0, false);
                            }
                            tier.under.paint(window, origin, v.k, Some(bounds));
                            tier.layer.paint(window, origin, v.k, Some(bounds));
                            // Ink is already on textures, so tracing someone veils the board and paints
                            // their parts again on top, which costs next to nothing to turn on and off.
                            if let Some((tone, shown, marked)) = trace {
                                let on_screen = |area: &[f32; 4]| Bounds::new(origin + point(px(area[0] * v.k), px(area[1] * v.k)), size(px(area[2] * v.k), px(area[3] * v.k)));
                                let mut veil: Hsla = rgb(pal().paper).into();
                                veil.a = TRACE_VEIL;
                                window.paint_quad(gpui_kit::fill(bounds, veil));
                                let mut mark: Hsla = rgb(tone).into();
                                mark.a = TRACE_MARK;
                                for area in &marked {
                                    // The same highlighter the board's own marks use.
                                    let middle = area[1] + area[3] / 2.0;
                                    let band = marker(&[[area[0], middle], [area[0] + area[2], middle]], area[3], 0.0, &mut Rng::new("trace"));
                                    let shape = outline(&band, 1.0);
                                    let mut pb = fill_builder();
                                    let at = |p: &Pt| origin + point(px(p[0] * v.k), px(p[1] * v.k));
                                    if let Some(first) = shape.first() {
                                        pb.move_to(at(first));
                                        shape[1..].iter().for_each(|p| pb.line_to(at(p)));
                                        pb.close();
                                    }
                                    if let Ok(path) = pb.build() {
                                        window.paint_path(path, Background::from(mark));
                                    }
                                }
                                for area in &shown {
                                    let patch = on_screen(area);
                                    window.with_content_mask(Some(gpui_kit::ContentMask { bounds: patch }), |window| {
                                        tier.under.paint(window, origin, v.k, Some(patch));
                                        tier.layer.paint(window, origin, v.k, Some(patch));
                                    });
                                }
                            }
                            // The circles, in the accent's ink: an aid for pointing, never part of the board.
                            for shape in b.loops.iter().chain(b.drawing.iter()) {
                                let mut pb = PathBuilder::stroke(px(2.5));
                                let at = |p: &[f32; 2]| origin + point(px(p[0] * v.k), px(p[1] * v.k));
                                if let Some(first) = shape.first() {
                                    pb.move_to(at(first));
                                    shape[1..].iter().for_each(|p| pb.line_to(at(p)));
                                    if b.drawing.as_ref() != Some(shape) {
                                        pb.close();
                                    }
                                }
                                if let Ok(path) = pb.build() {
                                    let mut ink: Hsla = rgb(pal().accent).into();
                                    ink.a = 0.8;
                                    window.paint_path(path, Background::from(ink));
                                }
                            }
                            // Ink making room for a new line, on its way to where it now belongs.
                            if let Some(left) = b.slide_left() {
                                for (stroke, from) in &b.sliding {
                                    let (dx, dy) = (from[0] * left, from[1] * left);
                                    let mut pb = fill_builder();
                                    let at = |p: &Pt| origin + point(px((p[0] + dx) * v.k), px((p[1] + dy) * v.k));
                                    if let Some(first) = stroke.shape.first() {
                                        pb.move_to(at(first));
                                        stroke.shape[1..].iter().for_each(|p| pb.line_to(at(p)));
                                        pb.close();
                                    }
                                    if let Ok(path) = pb.build() {
                                        window.paint_path(path, Background::from(color(stroke.style)));
                                    }
                                }
                            }
                            if let Some(front) = b.queue.front().filter(|_| b.live > 0.0) {
                                let shape = outline(&front.inked, b.live);
                                let mut pb = fill_builder();
                                if let Some(first) = shape.first() {
                                    let at = |p: &Pt| origin + point(px(p[0] * v.k), px(p[1] * v.k));
                                    pb.move_to(at(first));
                                    for p in &shape[1..] {
                                        pb.line_to(at(p));
                                    }
                                    pb.close();
                                }
                                if let Ok(path) = pb.build() {
                                    window.paint_path(path, Background::from(color(front.style)));
                                }
                            }
                        });
                        b.note_frame(started.elapsed(), uploads_before);
                    },
                )
                .size_full(),
            );
        let speakers = self.live.speakers();
        self.ink.borrow_mut().trace = self.traced_speaker().map(|who| (who.clone(), speaker_marker(&speakers, who)));
        let toolbar = {
            let ink = self.ink.borrow();
            ink.toolbar_at().zip(ink.selected.clone())
        };
        if let Some(((x, y), id)) = toolbar.filter(|_| self.editable()) {
            let has_text = self.live.graph.nodes.iter().find(|n| n.id == id).is_some_and(|n| !n.rows().is_empty());
            let fixed = self.live.graph.nodes.iter().find(|n| n.id == id).is_some_and(|n| n.kind == Kind::Region);
            let editing = self.ink.borrow().editing;
            let raised = self.live.graph.nodes.iter().find(|n| n.id == id).and_then(|n| by_of_node(n)).map(str::to_string);
            let unit_now = self.ink.borrow().mark_unit;
            let active: Vec<String> = self
                .ink
                .borrow()
                .graph
                .as_ref()
                .and_then(|g| g.nodes.iter().find(|n| n.id == id))
                .map_or(vec![], |n| n.marks.clone());
            board = board.child(
                div()
                    .id("marks")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .h(px(TOOLBAR_H))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(pal().line))
                    .bg(rgb(pal().surface))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(editing, |d| d.child(div().w(px(TOOLBAR_W_EDITING - 16.0)).child(Input::new(&self.label_input).small())))
                    .when_some(raised.clone().filter(|_| !editing), |d, who| {
                        let (speakers, source) = (self.live.speakers(), id.clone());
                        d.child(
                            div()
                                .id("raised-by")
                                .flex()
                                .items_center()
                                .gap_1()
                                .px_1()
                                .rounded_sm()
                                .cursor_pointer()
                                .text_xs()
                                .child(speaker_badge(&speakers, &who))
                                .child(div().max_w(px(70.)).overflow_hidden().whitespace_nowrap().child(who))
                                .on_click(cx.listener(move |this, _, _, cx| this.jump_to_source(&source, cx))),
                        )
                    })
                    .when(has_text && !editing, |d| {
                        let units = [(MarkUnit::Word, "unit-word", tr("Word", "词")), (MarkUnit::Sentence, "unit-sentence", tr("Sentence", "句")), (MarkUnit::Block, "unit-block", tr("Block", "段"))];
                        d.children(units.into_iter().map(|(unit, name, label)| {
                            let chooser = self.ink.clone();
                            div()
                                .id(name)
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .cursor_pointer()
                                .text_xs()
                                .border_1()
                                .border_color(rgb(pal().line))
                                .when(unit_now == unit, |d| d.bg(rgb(pal().selected)))
                                .child(label)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    chooser.borrow_mut().mark_unit = unit;
                                    cx.notify();
                                }))
                        }))
                    })
                    .children(MARKS.iter().filter(|_| !editing).map(|(mark, english)| {
                        let on = active.iter().any(|m| m == mark);
                        let id = id.clone();
                        let label = if tr("en", "zh") == "zh" { *mark } else { *english };
                        div()
                            .id(*mark)
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_sm()
                            .when(on, |d| d.bg(rgb(pal().strong)).text_color(rgb(pal().on_strong)))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| this.mark(&id, mark, cx)))
                    }))
                    // Imported text stays as it was printed, so only what was written can change.
                    .when(!fixed && !editing, |d| {
                        let (rewrite, removed) = (id.clone(), id.clone());
                        d.child(Separator::vertical().h(px(18.)))
                            .child(icon_button("rewrite", "pencil", tr("Change the words", "改字")).on_click(cx.listener(move |this, _, window, cx| this.start_label(&rewrite, window, cx))))
                            .child(icon_button("remove", "trash", tr("Remove, with what hangs under it", "删除（连同下面挂着的）")).on_click(cx.listener(move |this, _, _, cx| this.remove(&removed, cx))))
                    }),
            );
        }
        let editable = self.editable();
        let (page, pages) = (self.live.page(), self.live.pages());
        let save_label: String = match &self.save {
            _ if self.store.is_none() => tr("Sample, not saved", "示例，不保存").into(),
            // The replay bar says so already.
            _ if self.replay.is_some() => String::new(),
            _ if !editable => tr("View only", "只读").into(),
            // Saving is the normal case and takes a moment, so only a failure is worth a word.
            SaveState::Idle | SaveState::Dirty | SaveState::Saving | SaveState::Saved => String::new(),
            SaveState::Failed(_) => tr("Not saved", "保存失败").into(),
        };
        // Pages and dialogs cover the board; the side keeps the transcript beside them.
        let side_panel = if self.panel.beside_board() { self.panel } else { Panel::Transcript };
        let side_shown = side_panel != Panel::Transcript || self.transcript_open;
        // Nothing floats over the board any more but its corner label, so it may use the whole canvas.
        self.ink.borrow_mut().set_insets([BOARD_MARGIN, BOARD_MARGIN + 16.0, BOARD_MARGIN, BOARD_MARGIN]);
        let recording = self.pipeline.listening();
        let recorded = self.recorded();
        // Whether what was said has reached the board: the question to answer at a glance.
        let spoken_lines = || self.live.lines.iter().enumerate().filter(|(_, l)| !l["manual"].as_bool().unwrap_or(false));
        let waiting = spoken_lines().filter(|(_, l)| l["ms"].is_null() && !l["failed"].as_bool().unwrap_or(false)).count();
        let not_drawn: Vec<usize> = spoken_lines().filter(|(_, l)| l["failed"].as_bool().unwrap_or(false)).map(|(i, _)| i).collect();
        // The window's own title bar, as on the website: the meeting on the left in plain words and figures,
        // and what to open on the right.
        let mono = |text: String| div().font_family(theme::MONO).text_xs().whitespace_nowrap().child(text);
        let (state_text, state_color, state_line) = match () {
            _ if self.replay.is_some() => (String::new(), pal().faint, None),
            _ if !editable => (tr("view only", "只读").to_string(), pal().faint, None),
            _ if !not_drawn.is_empty() => (format!("{} {}", not_drawn.len(), tr("not drawn: show", "句没画上：查看")), pal().danger, not_drawn.last().copied()),
            _ if self.needs_model => (tr("no board model yet", "还没有板书模型").to_string(), pal().warning, None),
            _ if waiting > 0 && self.preparing.is_some() => (format!("{waiting} {}", tr("waiting for the model", "句等模型就绪")), pal().warning, None),
            _ if waiting > 1 => (format!("{} · {} {}", tr("drawing", "正在画"), waiting - 1, tr("waiting", "句排队")), pal().recording, None),
            _ if waiting == 1 => (tr("drawing", "正在画").to_string(), pal().recording, None),
            // Listening shows in the red clock beside it, and a ready board needs no word.
            _ => (String::new(), pal().faint, None),
        };
        let started = self.live.created_at().map(local_time).filter(|t| !t.is_empty()).map(|t| t.rsplit(' ').next().unwrap_or_default().to_string());
        let meeting_side = div()
            .flex()
            .items_center()
            .gap_5()
            .min_w_0()
            .map(|d| {
                if self.editing_title {
                    d.child(div().w(px(280.)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(Input::new(&self.title_input).small()))
                } else {
                    d.child(
                        div()
                            .id("meeting-title")
                            .max_w(px(320.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_base()
                            .when(editable, |d| d.cursor_pointer().hover(|d| d.text_color(rgb(pal().muted))))
                            .child(self.live.title().to_string())
                            .when_some(started.clone(), |d, started| {
                                let tip = format!("{} {started}", tr("Started at", "开始于"));
                                d.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| this.edit_title(window, cx))),
                    )
                }
            })
            .when(recording || !recorded.is_zero(), |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_color(rgb(if recording { pal().recording } else { pal().faint }))
                        .child(div().size(px(8.)).bg(rgb(if recording { pal().recording } else { pal().faint })))
                        .child(mono(clock(recorded.as_secs_f64() * 1000.0))),
                )
            })
            .when(!state_text.is_empty(), |d| d.child(
                div()
                    .id("board-state")
                    .flex()
                    .gap_1()
                    .when(state_line.is_some() || self.needs_model, |d| d.cursor_pointer())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(line) = state_line {
                            this.panel = Panel::Transcript;
                            this.transcript_open = true;
                            this.jumped_line = Some((line, Instant::now()));
                        } else if this.needs_model {
                            this.panel = Panel::Settings;
                        }
                        cx.notify();
                    }))
                    .text_xs()
                    .whitespace_nowrap()
                    .text_color(rgb(state_color))
                    .child(state_text),
            ))
            .when(!save_label.is_empty(), |d| {
                // A meeting that is not on disk is the one state here that must not go unseen.
                let failed = matches!(self.save, SaveState::Failed(_));
                d.child(div().text_xs().whitespace_nowrap().child(save_label).text_color(rgb(if failed { pal().danger } else { pal().faint })))
            });
        let open_side = div()
            .flex()
            .items_center()
            .gap_1()
            .pr_2()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when_some(match &self.update {
                UpdateState::Available(release) => Some(format!("{} {}", tr("Update to", "更新到"), release.version)),
                UpdateState::ReadyToRestart(_) => Some(tr("Restart to update", "重启完成更新").to_string()),
                _ => None,
            }, |d, label| {
                d.child(Button::new("update-chip").small().success().label(label).on_click(cx.listener(|this, _, _, cx| {
                    if matches!(this.update, UpdateState::ReadyToRestart(_)) {
                        this.restart(cx);
                    } else {
                        this.install_update(cx);
                    }
                })))
            })
            .when(pages > 1, |d| {
                d.child(icon_button("previous", "chevron-left", tr("Previous board", "上一张板")).on_click(cx.listener(move |this, _, _, cx| this.go_to_page(page.saturating_sub(1), cx))))
                    .child(mono(format!("{} / {pages}", page + 1)))
                    .child(icon_button("next", "chevron-right", tr("Next board", "下一张板")).on_click(cx.listener(move |this, _, _, cx| this.go_to_page(page + 1, cx))))
                    .child(Separator::vertical().h(px(18.)).mx_1())
            })
            .child(icon_button("library", "folder-open", tr("Meetings", "会议")).selected(self.panel == Panel::Library).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Library, cx))))
            .child(match self.store {
                Some(_) => icon_button("new", "file-plus", tr("New meeting", "新建会议")).on_click(cx.listener(|this, _, _, cx| this.new_meeting(cx))),
                // A sample has nowhere to keep a meeting, so the button says why instead of doing nothing.
                None => icon_button("new", "file-plus", tr("A sample is not saved, so it cannot start a new meeting", "示例不会保存，不能从这里新建会议")).disabled(true),
            })
            .child(icon_button("import", "import", tr("Import", "导入")).selected(self.panel == Panel::Import).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Import, cx))))
            .child(icon_button("export", "file-down", tr("Export", "导出")).selected(self.panel == Panel::Export).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Export, cx))))
            .child(Separator::vertical().h(px(18.)).mx_1())
            .child(icon_button("speakers", "users", tr("Speakers", "说话人")).selected(self.panel == Panel::Speakers).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Speakers, cx))))
            .child(icon_button("settings", "settings", tr("Settings", "设置")).selected(self.panel == Panel::Settings).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Settings, cx))))
            .child(icon_button("transcript-toggle", if self.transcript_open { "panel-right-close" } else { "panel-right-open" }, tr("Transcript", "转写")).on_click(cx.listener(|this, _, _, cx| {
                if this.panel == Panel::Transcript {
                    this.transcript_open = !this.transcript_open;
                } else {
                    this.panel = Panel::Transcript;
                    this.transcript_open = true;
                }
                cx.notify();
            })));
        let window_bar = TitleBar::new().bg(rgb(pal().surface)).border_color(rgb(pal().line)).child(meeting_side).child(open_side);

        // The dock along the board's lower edge: each command with its name; keys are told on hover.
        let key = |mac: &'static str, other: &'static str| if cfg!(target_os = "macos") { mac } else { other };
        let dock_button = |id: &'static str, icon_name: &'static str, label: String| {
            Button::new(id).ghost().rounded_none().h(px(DOCK_H - 1.0)).px_4().border_r_1().border_color(rgb(pal().line)).child(div().flex().items_center().gap_2().child(icon(icon_name).size_5()).child(div().text_sm().child(label)))
        };
        let (circling, circled) = {
            let ink = self.ink.borrow();
            (ink.circling, ink.pointed.len())
        };
        let dock = div()
            .flex()
            .items_center()
            .h(px(DOCK_H))
            .flex_none()
            .border_t_1()
            .border_color(rgb(pal().line))
            .bg(rgb(pal().surface))
            .when(editable, |d| {
                // The microphone sits with the other commands until it listens, then turns red with the level beside it.
                d.child(
                    Button::new("mic")
                        .map(|b| if listening { b.danger() } else { b.ghost() })
                        .border_r_1()
                        .border_color(rgb(pal().line))
                        .rounded_none()
                        .h(px(DOCK_H - 1.0))
                        .px_4()
                        .tooltip(if listening { tr("Stop listening (Space)", "停止收音（空格）") } else { tr("Start listening (Space)", "开始收音（空格）") })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_mic();
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(div().size(px(20.)).flex().items_center().justify_center().child(div().size(px(12.)).map(|d| if listening { d.bg(rgb(pal().on_strong)) } else { d.rounded_full().bg(rgb(pal().recording)) })))
                                .child(div().text_sm().child(if listening { tr("stop listening", "停止收音") } else { tr("start listening", "开始收音") }))
                                .when(listening, |d| {
                                    let level = self.level;
                                    d.child(div().flex().items_end().gap(px(2.)).h(px(24.)).children((0..7).map(|i| {
                                        let h = 3.0 + level * 21.0 * (0.55 + 0.45 * ((i as f32) * 1.7).sin().abs());
                                        div().w(px(3.)).h(px(h)).bg(rgb(pal().on_strong))
                                    })))
                                }),
                        ),
                )
                .child(
                    // While circling, the button counts what is circled; what to do is in its tooltip.
                    dock_button("circle", "lasso", match circled {
                        n if circling && n > 0 => format!("{} · {n}", tr("circle", "圈选")),
                        _ => tr("circle", "圈选").to_string(),
                    })
                        .selected(circling)
                        .tooltip(match (circling, circled) {
                            (false, _) => tr("Circle things on the board, then say what to do with them", "圈住板上的东西，再说要怎么处理"),
                            (true, 0) => tr("Draw around things on the board. Esc stops", "在板上画圈，Esc 取消"),
                            _ => tr("Now say what to do with them. Esc stops", "现在说要怎么处理，Esc 取消"),
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            let mut ink = this.ink.borrow_mut();
                            if ink.circling {
                                ink.clear_circles();
                            } else {
                                ink.circling = true;
                                ink.selected = None;
                            }
                            cx.notify();
                        })),
                )
            })
            // What changes the board, beside the tools that draw on it.
            .when(editable, |d| {
                d.child(div().w(px(8.)))
                    .child(icon_button("undo", "undo-2", key(tr("Undo (⌘Z)", "撤销（⌘Z）"), tr("Undo (Ctrl+Z)", "撤销（Ctrl+Z）"))).disabled(!self.live.can_undo()).on_click(cx.listener(|this, _, _, cx| this.step(false, cx))))
                    .child(icon_button("redo", "redo-2", key(tr("Redo (⇧⌘Z)", "重做（⇧⌘Z）"), tr("Redo (Ctrl+Shift+Z)", "重做（Ctrl+Shift+Z）"))).disabled(!self.live.can_redo()).on_click(cx.listener(|this, _, _, cx| this.step(true, cx))))
                    .child(icon_button("tidy", "wand-sparkles", tr("Tidy", "整理排版")).on_click(cx.listener(|this, _, _, cx| this.tidy(cx))))
            })
            // What is done with the meeting as a whole, at the far end.
            .child(div().flex_1())
            .child(dock_button("summary", "sparkles", tr("summary", "摘要").to_string()).border_r_0().border_l_1().selected(self.panel == Panel::Summary).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Summary, cx))))
            .child(
                dock_button("replay", "rotate-ccw", if self.replay.is_some() { tr("exit replay", "退出回放") } else { tr("replay", "回放") }.to_string())
                    .tooltip(if self.replay.is_some() { tr("Exit replay (Esc)", "退出回放（Esc）") } else { key(tr("Replay the meeting (⇧⌘R)", "回放会议（⇧⌘R）"), tr("Replay the meeting (Ctrl+Shift+R)", "回放会议（Ctrl+Shift+R）")) })
                    .border_r_0()
                    .border_l_1()
                    .selected(self.replay.is_some())
                    .disabled(self.live.steps.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.replay.is_some() {
                            this.exit_replay(cx);
                        } else {
                            this.start_replay(window, cx);
                        }
                    })),
            );
        // The corner of the board: how far it is zoomed, whether it is live, and what the model is doing.
        let coords = div()
            .flex()
            .items_center()
            .gap_2()
            .font_family(theme::MONO)
            .text_size(px(11.))
            .text_color(rgb(pal().faint))
            .whitespace_nowrap()
            .child(
                div()
                    .id("zoom")
                    .occlude()
                    .cursor_pointer()
                    .hover(|d| d.text_color(rgb(pal().text)))
                    .tooltip(|window, cx| Tooltip::new(if cfg!(target_os = "macos") { tr("Fit the board (⌘0). Zoom with ⌘+ and ⌘−", "适应屏幕（⌘0），⌘+ / ⌘− 缩放") } else { tr("Fit the board (Ctrl+0). Zoom with Ctrl+ and Ctrl−", "适应屏幕（Ctrl+0），Ctrl+ / Ctrl− 缩放") }).build(window, cx))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.fit_board(cx)))
                    .child(format!("{:.0}%", zoom * 100.0)),
            )
            // Following the talk is the usual state and goes unsaid; looking elsewhere offers the way back.
            .when(self.replay.is_some(), |d| d.child("·").child(tr("replay", "回放")))
            .when(reading && self.replay.is_none(), |d| {
                let back = self.ink.clone();
                d.child(
                    div()
                        .id("latest")
                        .occlude()
                        .cursor_pointer()
                        .ml_1()
                        .px_2()
                        .border_1()
                        .border_color(rgb(pal().line))
                        .bg(rgb(pal().surface))
                        .text_color(rgb(pal().text))
                        .hover(|d| d.border_color(rgb(pal().text)))
                        .child(tr("↓ back to latest", "↓ 回到最新"))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |_, _, _, cx| {
                            back.borrow_mut().show_latest();
                            cx.notify();
                        })),
                )
            })
            .when_some(self.status.text(), |d, text| d.child("·").child(text))
            .when_some(self.page_hint.clone().filter(|_| self.replay.is_none()), |d, (board, number, title)| {
                d.child(
                    div()
                        .id("page-hint")
                        .occlude()
                        .cursor_pointer()
                        .ml_2()
                        .px_2()
                        .border_1()
                        .border_color(rgb(pal().accent))
                        .text_color(rgb(pal().accent))
                        .hover(|d| d.text_color(rgb(pal().text)).border_color(rgb(pal().text)))
                        .tooltip(|window, cx| Tooltip::new(tr("This seems to be about another page. Click, or say “go there”.", "刚才说的像是另一页的内容。点一下，或者说「跳过去」。")).build(window, cx))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.page_quiet = QUIET_AFTER_TURNING;
                            this.go_to_page(board, cx);
                        }))
                        .child(tr("→ page {n} · {title}", "→ 第 {n} 页 · {title}").replace("{n}", &number.to_string()).replace("{title}", &title)),
                )
            })
            .when_some(self.topic_hint.clone().filter(|_| self.replay.is_none() && self.editable()), |d, (_, label)| {
                d.child(
                    div()
                        .id("topic-hint")
                        .occlude()
                        .cursor_pointer()
                        .ml_2()
                        .px_2()
                        .border_1()
                        .border_color(rgb(pal().accent))
                        .text_color(rgb(pal().accent))
                        .hover(|d| d.text_color(rgb(pal().text)).border_color(rgb(pal().text)))
                        .tooltip(|window, cx| Tooltip::new(tr("The talk moved on to a new topic. Click to give it a board of its own.", "讨论换到了新话题。点一下，把它移到新的一张板上。")).build(window, cx))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, _, cx| this.split_topic(cx)))
                        .child(tr("new topic “{t}” → new board", "新话题「{t}」→ 新开一张板").replace("{t}", &label)),
                )
            });
        // With the transcript away, the latest line still shows that the room is being heard.
        let transcript_shown = self.panel == Panel::Transcript && self.transcript_open;
        let caption = self.live.lines.iter().rev().find(|l| !l["manual"].as_bool().unwrap_or(false)).and_then(|l| l["text"].as_str()).map(str::to_string).filter(|_| !transcript_shown && self.replay.is_none());
        let banner = self.notice.clone().or_else(|| match &self.save {
            SaveState::Failed(error) => Some(format!("{}{error}", tr("Your meeting is still open here, but it was not saved: ", "这场会还在这里，但没有保存成功："))),
            _ => None,
        });
        let view_only = self.live.read_only.map(|reason| match reason {
            ViewOnly::Unreadable => tr("This meeting's board could not be read. It is view-only and its file is unchanged.", "这场会的板书读取失败，只能查看，文件不会被改动。"),
            ViewOnly::NewerFormat => tr("This meeting was saved by a newer version, so it is view-only.", "这场会是新版本保存的，只能查看。"),
            ViewOnly::ImportedContent => tr("This meeting has free-edit content, which this version does not show. It is view-only here, and its file is unchanged.", "这场会含自由编辑内容，这个版本不显示，这里只能查看，文件不会被改动。"),
        });
        // During a replay the transcript has only reached that moment.
        let shown_lines = self.replay.as_ref().map_or(self.live.lines.len(), |r| r.player.lines_said(&r.steps).min(self.live.lines.len()));
        let replay_bar = self.replay_bar(window, cx);
        let transcript = div()
            .id("transcript")
            .track_scroll(&self.transcript_scroll)
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .flex()
            .flex_col()
            .children(self.live.lines[..shown_lines].iter().enumerate().map(|(i, l)| {
                let manual = l["manual"].as_bool().unwrap_or(false);
                // The badge is drawn in outline until the model has put the line on the board.
                let drawn = !l["ms"].is_null() && !l["failed"].as_bool().unwrap_or(false);
                let text = l["text"].as_str().unwrap_or_default();
                let typing = self.typing.as_ref().filter(|t| t.line == i && t.animate).map(Typing::chars).filter(|&n| n < text.chars().count());
                let text = match typing {
                    Some(n) => format!("{}▍", text.chars().take(n).collect::<String>()),
                    None => text.to_string(),
                };
                let note = line_note(l, self.preparing.is_some());
                let jumped = self.jumped_line.is_some_and(|(line, _)| line == i);
                div()
                    .flex()
                    .flex_col()
                    .when(jumped, |d| d.rounded_md().bg(rgb(pal().jumped)))
                    .when_some(l["speaker"].as_str().filter(|s| !s.is_empty()).map(str::to_string), |d, name| {
                        let (picked, hovered) = (name.clone(), name.clone());
                        d.child(
                            div()
                                .id(("line-speaker", i))
                                .on_hover(cx.listener(move |this, over: &bool, _, cx| this.hover_speaker(&hovered, *over, cx)))
                                .flex()
                                .items_center()
                                .gap_1()
                                .mb_1()
                                .cursor_pointer()
                                .child(line_badge(&speakers, &name, drawn))
                                .child(div().text_xs().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(name))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.choose_speaker(picked.clone(), window, cx);
                                    this.speaker_line = Some(i);
                                })),
                        )
                    })
                    .child(div().id(("line-text", i)).text_sm().when(manual, |d| d.text_color(rgb(pal().faint))).child(text).when_some(line_timing(l), |d, tip| d.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))))
                    .when(!note.is_empty() && !manual, |d| {
                        d.child(div().text_xs().text_color(rgb(if l["failed"].as_bool().unwrap_or(false) { pal().danger } else { pal().faint })).child(note))
                    })
                    .when(l["failed"].as_bool().unwrap_or(false) && editable, |d| {
                        d.child(div().mt_1().child(button(("retry-line", i), tr("Draw it again", "重新上板")).on_click(cx.listener(move |this, _, _, cx| this.retry_line(i, cx)))))
                    })
            }));
        let typed = self.library_query.read(cx).value().to_string();
        self.search_typed(&typed);
        let query = typed.trim().to_lowercase();
        let mut found: Vec<(usize, &Info)> = self.library.iter().enumerate().filter(|(_, m)| query.is_empty() || m.title.to_lowercase().contains(&query)).collect();
        // Pinned meetings first, in the order they were pinned; the rest stay newest first.
        let pin_rank = |id: &str| self.pinned.iter().position(|p| p == id).unwrap_or(usize::MAX);
        found.sort_by_key(|(_, m)| pin_rank(&m.id));
        let library = div()
            .id("library")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            // A handful of meetings reads at a glance; past that, a search finds one.
            .when(self.library.len() > LIBRARY_SEARCH_FROM || self.search.is_some(), |d| d.child(div().pb_3().child(Input::new(&self.library_query).small().cleanable(true))))
            .child(
                div()
                    .id("library-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .pb_8()
                    .flex()
                    .flex_col()
                    .when(self.store.is_none(), |d| {
                        d.child(div().p_2().text_sm().text_color(rgb(pal().faint)).child(tr("This is a sample, which is not saved, so there are no meetings to list here.", "这是示例，不会保存，这里没有会议可以列出。")))
                    })
                    .when(self.store.is_some() && query.is_empty(), |d| {
                        let own: Vec<(String, Vec<String>, Option<usize>)> = templates()
                            .into_iter()
                            .map(|(n, t)| (n.to_string(), t, None))
                            .chain(self.form.saved.templates.iter().enumerate().map(|(i, (n, t))| (n.clone(), t.clone(), Some(i))))
                            .collect();
                        let has_topics = self.live.graph.nodes.iter().any(|n| n.kind == Kind::Topic);
                        let section = |name: &'static str| div().pt_2().pb_2().text_xs().font_weight(gpui_kit::FontWeight::SEMIBOLD).text_color(rgb(pal().muted)).child(name);
                        // Templates as cards in a row: what a new meeting starts with, at a glance.
                        let card = || div().w(px(220.)).h(px(78.)).p_3().flex().flex_col().gap_1().border_1().border_color(rgb(pal().line)).rounded(px(3.)).cursor_pointer();
                        d.child(section(tr("Start from a template", "从模板新建")))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .pb_5()
                                    .children(own.into_iter().enumerate().map(|(i, (name, topics, saved))| {
                                        let shown = topics.join(" · ");
                                        card()
                                            .id(("template", i))
                                            .bg(rgb(pal().surface))
                                            .hover(|d| d.border_color(rgb(pal().text)))
                                            .on_click(cx.listener(move |this, _, _, cx| this.new_meeting_from(topics.clone(), cx)))
                                            .child(
                                                div()
                                                    .flex()
                                                    .justify_between()
                                                    .child(div().text_sm().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(name))
                                                    .when_some(saved, |d, index| {
                                                        d.child(
                                                            div()
                                                                .id(("forget-template", index))
                                                                .px_1()
                                                                .text_xs()
                                                                .text_color(rgb(pal().faint))
                                                                .hover(|d| d.text_color(rgb(pal().text)))
                                                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    cx.stop_propagation();
                                                                    this.forget_template(index, cx)
                                                                }))
                                                                .child("×"),
                                                        )
                                                    }),
                                            )
                                            .child(div().text_xs().text_color(rgb(pal().muted)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(shown))
                                    }))
                                    .when(has_topics, |d| {
                                        d.child(
                                            card()
                                                .id("save-template")
                                                .border_dashed()
                                                .border_color(rgb(pal().faint))
                                                .items_center()
                                                .justify_center()
                                                .text_xs()
                                                .text_color(rgb(pal().muted))
                                                .hover(|d| d.text_color(rgb(pal().text)).border_color(rgb(pal().text)))
                                                .on_click(cx.listener(|this, _, _, cx| this.save_template(cx)))
                                                .child(tr("+ Keep this board's topics as a template", "+ 把这张板的话题存成模板")),
                                        )
                                    }),
                            )
                            .child(section(tr("Meetings", "会议")).border_b_1().border_color(rgb(pal().line)))
                    })
                    .when(self.store.is_some() && self.library.is_empty(), |d| {
                        d.child(div().py_4().text_sm().text_color(rgb(pal().faint)).child(tr("No meetings yet. A meeting is saved here as soon as something is said or typed.", "还没有会议。开始说话或输入后，会议会自动保存在这里。")))
                    })
                    // What matches by meaning, under the meetings whose names do: first where things stand (the
                    // boards and summaries as the meetings left them), then what was said on the way.
                    .when_some(self.search.as_ref().filter(|s| !s.hits.is_empty() && !query.is_empty()).map(|s| s.hits.clone()), |d, hits| {
                        let row = |i: usize, found: boardcast_search::service::Found, cx: &mut Context<Self>| {
                            let place = match &found.source {
                                boardcast_search::Source::Transcript { .. } => tr("transcript", "转写"),
                                boardcast_search::Source::Board { .. } => tr("board", "板书"),
                                boardcast_search::Source::Summary { .. } => tr("summary", "摘要"),
                            };
                            let snippet: String = found.text.split('\n').map(|l| l.trim()).collect::<Vec<_>>().join("  ").chars().take(180).collect();
                            let open = found.clone();
                            div()
                                .id(("found", i))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .px_3()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(pal().line))
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(pal().selected)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(div().text_sm().overflow_hidden().whitespace_nowrap().child(found.title.clone()))
                                        .child(div().font_family(theme::MONO).text_xs().text_color(rgb(pal().faint)).child(place)),
                                )
                                .child(div().text_xs().text_color(rgb(pal().muted)).child(snippet))
                                .on_click(cx.listener(move |this, _, _, cx| this.open_found(&open, cx)))
                        };
                        let heading = |name: &'static str| div().pt_5().pb_2().text_xs().font_weight(gpui_kit::FontWeight::SEMIBOLD).text_color(rgb(pal().muted)).border_b_1().border_color(rgb(pal().line)).child(name);
                        let (stands, said): (Vec<_>, Vec<_>) = hits.into_iter().enumerate().partition(|(_, f)| f.stands());
                        d.when(!stands.is_empty(), |d| d.child(heading(tr("Where things stand", "板上结论"))).children(stands.into_iter().map(|(i, f)| row(i, f, cx))))
                            .when(!said.is_empty(), |d| d.child(heading(tr("What was said", "讨论原话"))).children(said.into_iter().map(|(i, f)| row(i, f, cx))))
                    })
                    .when(self.search.as_ref().is_some_and(|s| s.asking) && !query.is_empty(), |d| {
                        d.child(div().p_2().text_sm().text_color(rgb(pal().faint)).child(tr("Searching what was said…", "正在搜索会议内容…")))
                    })
                    .when(!self.library.is_empty() && found.is_empty() && self.search.as_ref().is_none_or(|s| s.hits.is_empty() && !s.asking), |d| {
                        d.child(div().p_2().text_sm().text_color(rgb(pal().faint)).child(if self.search.is_some() { tr("Nothing in the meetings' names or in what was said matches that.", "会议名称和会议内容里都没有找到相关的。") } else { tr("No meeting has that in its name.", "没有名称里含这些字的会议。") }))
                    })
                    .when_some(self.deleted_meeting.clone(), |d, (_, title)| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .my_2()
                                .px_3()
                                .py_2()
                                .bg(rgb(pal().selected))
                                .text_xs()
                                .child(format!("{}「{title}」", tr("Deleted ", "已删除")))
                                .child(button("restore-meeting", tr("Undo", "撤销")).on_click(cx.listener(|this, _, _, cx| this.restore_meeting(cx)))),
                        )
                    })
                    .children(found.into_iter().map(|(i, m)| {
                        let id = m.id.clone();
                        let current = id == self.live.id();
                        let pinned = self.pinned.iter().any(|p| *p == id);
                        let (pin_id, delete_id) = (id.clone(), id.clone());
                        let detail = match &m.read_error {
                            Some(error) => format!("{}{error}", tr("Could not be read, and the file is left as it is: ", "无法读取，文件保持原样：")),
                            None => format!("{} · {} {}", local_time(m.updated_at), m.lines, tr("lines", "句")),
                        };
                        // A row of a list: the name, and when and how long on the right.
                        div()
                            .id(("meeting", i))
                            .group("meeting-row")
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .px_3()
                            .py_3()
                            .border_b_1()
                            .border_color(rgb(pal().line))
                            .when(m.read_error.is_none(), |d| d.cursor_pointer().hover(|d| d.bg(rgb(pal().selected))))
                            .when(current, |d| d.bg(rgb(pal().selected)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .min_w_0()
                                    .when(current, |d| d.child(div().size(px(6.)).flex_none().rounded_full().bg(rgb(pal().recording))))
                                    .child(div().text_sm().overflow_hidden().whitespace_nowrap().when(m.read_error.is_some(), |d| d.text_color(rgb(pal().muted))).child(m.title.clone())),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .flex_none()
                                    .child(div().max_w(px(360.)).overflow_hidden().whitespace_nowrap().font_family(theme::MONO).text_xs().text_color(rgb(if m.read_error.is_some() { pal().danger } else { pal().faint })).child(detail))
                                    // Shown on hover, and the star stays once a meeting is pinned.
                                    .when(self.store.is_some(), |d| {
                                        d.child(
                                            div()
                                                .id(("pin-meeting", i))
                                                .p_1()
                                                .cursor_pointer()
                                                .when(!pinned, |d| d.opacity(0.).group_hover("meeting-row", |s| s.opacity(1.)))
                                                .child(icon("star").size_3().text_color(rgb(if pinned { pal().recording } else { pal().faint })))
                                                .tooltip(move |window, cx| Tooltip::new(if pinned { tr("Unpin", "取消置顶") } else { tr("Pin to the top", "置顶") }).build(window, cx))
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.pin_meeting(&pin_id, cx);
                                                })),
                                        )
                                    })
                                    // The meeting that is open cannot be deleted from under itself.
                                    .when(self.store.is_some() && !current, |d| {
                                        d.child(
                                            div()
                                                .id(("delete-meeting", i))
                                                .p_1()
                                                .cursor_pointer()
                                                .opacity(0.)
                                                .group_hover("meeting-row", |s| s.opacity(1.))
                                                .child(icon("trash").size_3().text_color(rgb(pal().faint)))
                                                .tooltip(|window, cx| Tooltip::new(tr("Delete", "删除")).build(window, cx))
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.delete_meeting(&delete_id, cx);
                                                })),
                                        )
                                    }),
                            )
                            .when(m.read_error.is_none(), |d| d.on_click(cx.listener(move |this, _, _, cx| this.open_meeting(&id, cx))))
                    })),
            );
        let field = |label: &'static str, input: &Entity<InputState>| div().flex().flex_col().gap_1().child(div().text_xs().text_color(rgb(pal().muted)).child(label)).child(Input::new(input));
        let local_only = self.live.local_only();
        let current_model = boardcast_models::current(&self.models);
        let recommended = boardcast_models::recommended(self.vram_mib);
        let using_remote = self.form.saved.enabled && self.form.saved.is_complete();
        let progress_row = self.download.as_ref().map(|d| {
            let text = match &d.progress {
                Some(p) if p.verifying => format!("{} {}", d.label, tr("checking what was already downloaded…", "正在检查已下载的部分…")),
                Some(p) => format!(
                    "{} {} / {}{}",
                    d.label,
                    gigabytes(p.done),
                    gigabytes(p.total),
                    if p.speed > 0 { format!(" · {:.1} MB/s", p.speed as f64 / 1e6) } else { String::new() },
                ),
                None => format!("{} {}", d.label, tr("starting…", "正在开始…")),
            };
            let fraction = d.progress.as_ref().map_or(0.0, |p| p.done as f32 / p.total.max(1) as f32);
            (text, fraction)
        });
        let local_models = boardcast_models::catalog().into_iter().enumerate().map(|(i, model)| {
            let installed = boardcast_models::installed(&model, &self.models);
            let active = !using_remote && current_model.as_ref().is_some_and(|p| p.ends_with(&model.file));
            let busy = self.download.is_some();
            let name = model.label.to_string();
            let size = gigabytes(model.size);
            let recommended = model.id == recommended;
            let (action, fetch) = (model.clone(), model.clone());
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_2()
                .when(i > 0, |d| d.border_t_1().border_color(rgb(pal().line)))
                // The name gives way, so a long one can never push its button out of reach.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().text_sm().child(name))
                                .child(div().font_family(theme::MONO).text_xs().text_color(rgb(pal().faint)).child(size))
                                .when(recommended, |d| d.child(div().px_1().text_xs().border_1().border_color(rgb(pal().recording)).text_color(rgb(pal().recording)).child(tr("recommended", "推荐")))),
                        )
                        .when_some(model.note, |d, (en, zh)| d.child(div().text_xs().text_color(rgb(pal().muted)).child(tr(en, zh)))),
                )
                .when(active, |d| d.child(div().flex().items_center().gap_1().text_xs().child(div().size(px(6.)).rounded_full().bg(rgb(pal().recording))).child(tr("In use", "使用中"))))
                .when(installed && !active, |d| {
                    d.child(button(("use-model", i), tr("Use", "使用")).on_click(cx.listener(move |this, _, _, cx| this.use_model(&action, cx))))
                })
                .when(!installed && !busy, |d| {
                    d.child(button(("get-model", i), tr("Download", "下载")).on_click(cx.listener(move |this, _, _, cx| this.start_download(fetch.clone(), cx))))
                })
        }).collect::<Vec<_>>();
        let forced = self.forced_backend;
        let key_hint = self.form.saved.key_hint();
        let tab = self.settings_tab;
        // One setting a row: its name and a line on what it does on the left, the control on the right.
        let row = |title: &'static str, hint: Option<&'static str>| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_6()
                .py_3()
                .border_b_1()
                .border_color(rgb(pal().line))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_sm().child(title))
                        .when_some(hint, |d, hint| d.child(div().text_xs().text_color(rgb(pal().muted)).child(hint))),
                )
        };
        // A few choices side by side, the chosen one filled.
        let segmented = |buttons: Vec<Button>| div().flex().flex_none().p(px(2.)).gap(px(2.)).border_1().border_color(rgb(pal().line)).rounded(px(3.)).children(buttons);
        let choice = |id: gpui_kit::ElementId, label: &'static str, on: bool| Button::new(id).small().label(label).map(|b| if on { b.primary() } else { b.ghost() });
        let heading = |title: &'static str, intro: Option<&'static str>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pb_2()
                .child(div().text_xl().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(title))
                .when_some(intro, |d, intro| d.child(div().text_sm().text_color(rgb(pal().muted)).child(intro)))
        };
        let page: gpui_kit::AnyElement = match tab {
            SettingsTab::Model => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(heading(
                    tr("Board model", "板书模型"),
                    Some(if self.needs_model {
                        tr("To draw the board, download a model that runs on this computer, or set up a remote one. Speech recognition is already included.", "要开始画板书，请下载一个在本机运行的模型，或者设置远程模型。语音识别已经内置。")
                    } else {
                        tr("What turns what is said into the board.", "把说的话变成板书的模型。")
                    }),
                ))
                .child(div().flex().child(segmented(vec![
                    choice("use-local".into(), tr("On this computer", "本机"), !self.form.remote).on_click(cx.listener(|this, _, _, cx| {
                        this.form.remote = false;
                        cx.notify();
                    })),
                    choice("use-remote".into(), tr("Remote", "远程"), self.form.remote).on_click(cx.listener(|this, _, _, cx| {
                        this.form.remote = true;
                        cx.notify();
                    })),
                ])))
                .when(!self.form.remote, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .border_1()
                            .border_color(rgb(pal().line))
                            .rounded(px(3.))
                            .bg(rgb(pal().surface))
                            .children(local_models),
                    )
                })
                .when_some(progress_row, |d, (text, fraction)| {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().flex_1().flex().flex_col().gap_1().child(div().text_xs().child(text)).child(div().h(px(4.)).w_full().bg(rgb(pal().track)).child(div().h_full().w(gpui_kit::relative(fraction)).bg(rgb(pal().strong)))))
                            .child(button("stop-download", tr("Pause", "暂停")).on_click(cx.listener(|this, _, _, cx| {
                                if let Some(download) = &this.download {
                                    download.cancel.store(true, Ordering::Relaxed);
                                }
                                cx.notify();
                            }))),
                    )
                })
                .when(self.form.remote, |d| {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_xs().text_color(rgb(pal().muted)).child(tr("Fill in", "一键填写")))
                            .children(REMOTE_PRESETS.iter().enumerate().map(|(i, (name, base, model))| {
                                button(("remote-preset", i), *name).on_click(cx.listener(move |this, _, window, cx| {
                                    this.form.base_url.update(cx, |s, cx| s.set_value(*base, window, cx));
                                    this.form.model.update(cx, |s, cx| s.set_value(*model, window, cx));
                                    this.form.key.update(cx, |s, cx| s.focus(window, cx));
                                    cx.notify();
                                }))
                            })),
                    )
                    .child(field(tr("API address", "接口地址"), &self.form.base_url))
                    .child(field(tr("Model", "模型"), &self.form.model))
                    .child(field(tr("API key", "API key"), &self.form.key))
                    .child(div().text_xs().text_color(rgb(pal().faint)).child(match key_hint {
                        Some(hint) => format!("{} {hint}{}", tr("Saved key:", "已保存的 key："), tr(". Leave empty to keep it.", "。留空则继续使用。")),
                        None => tr("No key saved. It is stored only on this computer.", "还没有保存 key。key 只存在这台电脑上。").to_string(),
                    }))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Button::new("save-settings").small().primary().label(tr("Save", "保存")).on_click(cx.listener(|this, _, window, cx| this.save_settings(window, cx))))
                            .child(button("test-settings", tr("Test", "测试连接")).on_click(cx.listener(|this, _, _, cx| this.test_settings(cx)))),
                    )
                })
                // Leaving the remote model takes effect on saving, like choosing it does.
                .when(!self.form.remote && self.form.saved.enabled, |d| {
                    d.child(div().flex().child(Button::new("save-local").small().primary().label(tr("Save", "保存")).on_click(cx.listener(|this, _, window, cx| this.save_settings(window, cx)))))
                })
                .when_some(self.form.note.clone(), |d, note| d.child(div().text_sm().child(note)))
                .when(forced, |d| d.child(div().text_xs().text_color(rgb(pal().warning)).child(tr("The command line chose the model, so these settings apply next launch.", "当前模型由命令行指定，这些设置下次启动生效。"))))
                .when(local_only, |d| d.child(div().text_xs().text_color(rgb(pal().warning)).child(tr("This meeting is local-only, so it keeps using the local model.", "这场会设置为只用本地模型，不会发送到远程。"))))
                .into_any_element(),
            SettingsTab::Speech => {
                let chosen = self.form.saved.input.clone();
                let inputs = std::iter::once((String::new(), tr("The system's default input", "系统默认输入").to_string()))
                    .chain(self.inputs.iter().map(|i| (i.id.clone(), i.name.clone())))
                    .chain((!chosen.is_empty() && !self.inputs.iter().any(|i| i.id == chosen)).then(|| (chosen.clone(), tr("The chosen input, not connected now", "选择的设备（现在没有连接）").to_string())));
                div()
                    .flex()
                    .flex_col()
                    .child(heading(tr("Speech", "语音"), Some(tr("What is heard, and how it is written down.", "听什么，以及怎么写下来。"))))
                    .child(div().pt_3().pb_1().text_sm().child(tr("Listen to", "收音来源")))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .border_1()
                            .border_color(rgb(pal().line))
                            .rounded(px(3.))
                            .bg(rgb(pal().surface))
                            .children(inputs.enumerate().map(|(i, (id, name))| {
                                let on = id == chosen;
                                div()
                                    .id(("input", i))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .py_2()
                                    .when(i > 0, |d| d.border_t_1().border_color(rgb(pal().line)))
                                    .cursor_pointer()
                                    .text_sm()
                                    .hover(|d| d.bg(rgb(pal().selected)))
                                    .child(div().size(px(8.)).flex_none().rounded_full().when(on, |d| d.bg(rgb(pal().recording))).when(!on, |d| d.border_1().border_color(rgb(pal().faint))))
                                    .child(div().min_w_0().overflow_hidden().whitespace_nowrap().child(name))
                                    .on_click(cx.listener(move |this, _, _, cx| this.choose_input(id.clone(), cx)))
                            })),
                    )
                    .when(cfg!(target_os = "linux"), |d| {
                        d.child(div().pt_1().text_xs().text_color(rgb(pal().muted)).child(tr(
                            "For the other side of an online meeting, choose the \"Monitor of\" input for the speakers it plays through.",
                            "要收线上会议里对方的声音，选播放它的扬声器对应的 “Monitor of” 输入。",
                        )))
                    })
                    .child(row(tr("Tell speakers apart by voice", "按声音区分说话人"), Some(tr("New voices are named A, B, C. Voice data stays in the meeting file on this computer.", "新的声音按 A、B、C 命名，声纹只保存在本机的会议文件里。"))).child(
                        Switch::new("diarize").checked(self.form.saved.diarize).accessibility_label(tr("Tell speakers apart by voice", "按声音区分说话人")).on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.form.saved.diarize = *checked;
                            if let Err(e) = this.form.saved.save() {
                                this.form.note = Some(format!("{e:#}"));
                            }
                            cx.notify();
                        })),
                    ))
                    .child(row(tr("Turn to the page being talked about", "讲到别的页时自动翻过去"), Some(tr("Otherwise the board's corner offers the page.", "关闭时只在画布角落给出建议。"))).child(
                        Switch::new("turn-pages").checked(self.form.saved.turn_pages).accessibility_label(tr("Turn to the page being talked about", "讲到别的页时自动翻过去")).on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.form.saved.turn_pages = *checked;
                            if let Err(e) = this.form.saved.save() {
                                this.form.note = Some(format!("{e:#}"));
                            }
                            cx.notify();
                        })),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .py_3()
                            .border_b_1()
                            .border_color(rgb(pal().line))
                            .child(div().text_sm().child(tr("Glossary", "术语表")))
                            .child(div().text_xs().text_color(rgb(pal().muted)).child(tr(
                                "Names as they should be written, apart by semicolons; ways they are misheard go after an equals sign: API = A P I.",
                                "名字应该怎么写，用分号隔开；听错的写法写在等号后面，例如：API = A P I。",
                            )))
                            .child(div().pt_1().child(Input::new(&self.form.glossary).small())),
                    )
                    .when(self.speech_helper.is_some(), |d| {
                        let system = self.form.saved.system_speech;
                        let language = self.form.saved.speech_language.clone();
                        let languages = [("auto", tr("System", "跟随系统")), ("ja-JP", "日本語"), ("en-US", "English"), ("zh-CN", "中文")];
                        d.child(row(tr("Recognize speech with", "语音识别"), None).child(segmented(vec![
                            choice("speech-models".into(), tr("Speech models", "语音模型"), !system).on_click(cx.listener(|this, _, _, cx| this.choose_speech(false, None, cx))),
                            choice("speech-apple".into(), "Apple", system).on_click(cx.listener(|this, _, _, cx| this.choose_speech(true, None, cx))),
                        ])))
                        .when(system, |d| {
                            d.child(row(tr("Language heard", "识别语言"), Some(tr("Apple's recognizer hears one language at a time.", "Apple 的识别一次只听一种语言。"))).child(segmented(
                                languages.into_iter().map(|(id, label)| choice(gpui_kit::ElementId::Name(format!("speech-language-{id}").into()), label, language == id).on_click(cx.listener(move |this, _, _, cx| this.choose_speech(true, Some(id), cx)))).collect(),
                            )))
                        })
                    })
                    .when_some(self.hearing.clone(), |d, with| d.child(div().pt_2().text_xs().text_color(rgb(pal().faint)).child(format!("{} {with}", tr("Hearing with", "当前识别：")))))
                    .into_any_element()
            }
            SettingsTab::General => {
                let language = self.form.saved.interface_language.clone();
                let look = self.form.saved.appearance.clone();
                div()
                    .flex()
                    .flex_col()
                    .child(heading(tr("General", "通用"), None))
                    .child(row(tr("Language", "界面语言"), Some(tr("The board and summary follow the meeting's own language.", "板书和摘要跟随会议本身的语言。"))).child(segmented(
                        [("auto", tr("System", "跟随系统")), ("en", "English"), ("zh", "中文")]
                            .into_iter()
                            .map(|(id, label)| choice(gpui_kit::ElementId::Name(format!("interface-language-{id}").into()), label, language == id).on_click(cx.listener(move |this, _, _, cx| this.choose_language(id, cx))))
                            .collect(),
                    )))
                    .child(row(tr("Appearance", "外观"), None).child(segmented(
                        [("auto", tr("System", "跟随系统")), ("light", tr("Light", "浅色")), ("dark", tr("Dark", "深色"))]
                            .into_iter()
                            .map(|(id, label)| choice(gpui_kit::ElementId::Name(format!("appearance-{id}").into()), label, look == id).on_click(cx.listener(move |this, _, _, cx| this.choose_appearance(id, cx))))
                            .collect(),
                    )))
                    .child(row(
                        tr("Handwriting", "字迹"),
                        Some(tr("A messier hand leans and rounds its corners; further on it joins its strokes, and writes small English letters joined up.", "越潦草，字越歪、转角越圆；再往后笔画相连，英文小写也连起来写。")),
                    ).child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap_2()
                            .w(px(240.))
                            .child(div().text_xs().text_color(rgb(pal().muted)).child(tr("Neat", "工整")))
                            .child(Slider::new(&self.form.messy).accessibility_label(tr("How messy the handwriting is", "字迹潦草程度")).flex_1().min_w_0())
                            .child(div().text_xs().text_color(rgb(pal().muted)).child(tr("Messy", "潦草"))),
                    ))
                    .child(row(
                        tr("Search meetings by meaning", "按意思搜索会议"),
                        Some(tr("Finds what was said in any meeting from a question in your own words. Needs a 334 MB model that runs on this computer.", "用自己的话提问，就能在所有会议里找到相关的那段。需要下载一个 334 MB 的本机模型。")),
                    ).child(
                        Switch::new("meeting-search").checked(self.form.saved.meeting_search).accessibility_label(tr("Search meetings by meaning", "按意思搜索会议")).on_click(cx.listener(|this, on: &bool, _, cx| this.set_meeting_search(*on, cx))),
                    ))
                    .when_some(self.search_status(), |d, (text, failed)| d.child(div().pt_1().text_xs().text_color(rgb(if failed { pal().danger } else { pal().muted })).child(text)))
                    .into_any_element()
            }
            SettingsTab::About => {
                let update: gpui_kit::AnyElement = match &self.update {
                    _ if self.prefix.is_none() => div().text_xs().text_color(rgb(pal().muted)).child(tr("Installed copies update.", "只有安装版可以更新。")).into_any_element(),
                    UpdateState::Idle => button("check-update", tr("Check for updates", "检查更新")).on_click(cx.listener(|this, _, _, cx| { this.check_for_update(); cx.notify(); })).into_any_element(),
                    UpdateState::Checking => div().text_sm().text_color(rgb(pal().muted)).child(tr("Checking…", "正在检查…")).into_any_element(),
                    UpdateState::UpToDate => div().flex().items_center().gap_2().child(div().text_sm().text_color(rgb(pal().muted)).child(tr("Up to date", "已是最新"))).child(button("check-update", tr("Check again", "再次检查")).on_click(cx.listener(|this, _, _, cx| { this.check_for_update(); cx.notify(); }))).into_any_element(),
                    UpdateState::Available(release) => Button::new("install-update").small().primary().label(format!("{} {} · {}", tr("Install", "安装"), release.version, gigabytes(release.size))).on_click(cx.listener(|this, _, _, cx| this.install_update(cx))).into_any_element(),
                    UpdateState::Installing { release, progress } => {
                        let text = progress.as_ref().map_or(tr("Starting…", "正在开始…").to_string(), |p| {
                            if p.done >= p.total { tr("Verifying and installing…", "正在校验并安装…").to_string() } else { format!("{} / {}", gigabytes(p.done), gigabytes(p.total)) }
                        });
                        let fraction = progress.as_ref().map_or(0.0, |p| p.done as f32 / p.total.max(1) as f32);
                        div().w(px(200.)).flex().flex_col().gap_1()
                            .child(div().text_xs().child(format!("{} {text}", release.version)))
                            .child(div().h(px(4.)).w_full().bg(rgb(pal().track)).child(div().h_full().w(gpui_kit::relative(fraction)).bg(rgb(pal().strong))))
                            .into_any_element()
                    }
                    UpdateState::ReadyToRestart(_) => Button::new("restart-update").small().primary().label(tr("Save and restart", "保存并重启")).on_click(cx.listener(|this, _, _, cx| this.restart(cx))).into_any_element(),
                    UpdateState::Failed(_) => button("check-update", tr("Try again", "重试")).on_click(cx.listener(|this, _, _, cx| { this.check_for_update(); cx.notify(); })).into_any_element(),
                };
                let update_hint: Option<String> = match &self.update {
                    UpdateState::Available(release) if !release.notes.is_empty() => Some(release.notes.clone()),
                    UpdateState::ReadyToRestart(release) => Some(format!("{} {}", release.version, tr("is installed. Your meeting keeps running until you restart.", "已安装。重启之前，当前会议不受影响。"))),
                    UpdateState::Failed(error) => Some(error.clone()),
                    _ => None,
                };
                let failed = matches!(self.update, UpdateState::Failed(_));
                div()
                    .flex()
                    .flex_col()
                    .child(heading(tr("About", "关于"), None))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_6()
                            .py_3()
                            .border_b_1()
                            .border_color(rgb(pal().line))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(div().text_sm().child(format!("boardcast {}", boardcast_update::VERSION)))
                                    .when_some(update_hint, |d, hint| d.child(div().text_xs().text_color(rgb(if failed { pal().danger } else { pal().muted })).child(hint))),
                            )
                            .child(update),
                    )
                    // Metal keeps its own shaders, so there is nothing here for a Mac to manage.
                    .when(cfg!(not(target_os = "macos")), |d| {
                        let size = dir_size(&shader_dir(&self.models));
                        d.child(row(tr("Graphics cache", "显卡缓存"), Some(tr("Safe to delete; the model compiles it again in about half a minute.", "可以放心删除，下次启动模型时约半分钟重新编译。"))).child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(div().font_family(theme::MONO).text_xs().text_color(rgb(pal().muted)).child(format!("{:.1} MB", size as f64 / 1e6)))
                                .child(button("clear-shaders", tr("Delete", "删除")).disabled(size == 0).on_click(cx.listener(|this, _, _, cx| {
                                    if let Err(e) = std::fs::remove_dir_all(shader_dir(&this.models)) {
                                        this.form.note = Some(format!("{e:#}"));
                                    }
                                    cx.notify();
                                }))),
                        ))
                    })
                    .into_any_element()
            }
        };
        // Before there is a model, choosing one is the only thing to do here.
        let tabs: Vec<(SettingsTab, &'static str)> = [
            (SettingsTab::Model, tr("Board model", "板书模型")),
            (SettingsTab::Speech, tr("Speech", "语音")),
            (SettingsTab::General, tr("General", "通用")),
            (SettingsTab::About, tr("About", "关于")),
        ]
        .into_iter()
        .filter(|(t, _)| !self.needs_model || matches!(t, SettingsTab::Model | SettingsTab::General))
        .collect();
        let settings = div()
            .id("settings-page")
            .occlude()
            .absolute()
            .size_full()
            .flex()
            .bg(rgb(pal().paper))
            .child(
                div()
                    .w(px(200.))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p_3()
                    .pt_6()
                    .border_r_1()
                    .border_color(rgb(pal().line))
                    .bg(rgb(pal().surface))
                    .child(div().px_2().pb_3().text_xs().font_family(theme::MONO).text_color(rgb(pal().faint)).child(tr("settings", "设置")))
                    .children(tabs.into_iter().map(|(t, label)| {
                        let on = t == tab;
                        div()
                            .id(label)
                            .px_2()
                            .py(px(6.))
                            .cursor_pointer()
                            .text_sm()
                            .rounded(px(2.))
                            .when(on, |d| d.bg(rgb(pal().selected)).font_weight(gpui_kit::FontWeight::SEMIBOLD))
                            .when(!on, |d| d.text_color(rgb(pal().muted)).hover(|d| d.text_color(rgb(pal().text))))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings_tab = t;
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .id("settings-page-body")
                    .flex_1()
                    .h_full()
                    .overflow_y_scroll()
                    .child(div().max_w(px(640.)).px_10().pt_6().pb_10().child(page)),
            )
            .child(div().absolute().top(px(10.)).right(px(10.)).child(icon_button("close-settings", "x", tr("Close (Esc)", "关闭（Esc）")).on_click(cx.listener(|this, _, _, cx| {
                this.panel = Panel::Transcript;
                cx.notify();
            }))));
        let summary_panel = (self.panel == Panel::Summary).then(|| self.summary_panel(editable, cx));
        let export_panel = (self.panel == Panel::Export).then(|| self.export_panel(cx));
        let import_panel = (self.panel == Panel::Import).then(|| self.import_panel(cx));
        let speakers_panel = (self.panel == Panel::Speakers).then(|| self.speakers_panel(editable, cx));
        let side = div()
            .id("side")
            .occlude()
            .w(px(SIDE_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(pal().surface))
            .border_l_1()
            .border_color(rgb(pal().line))
            // Every panel says what it is and closes the same way.
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pl_4()
                    .pr_1()
                    .h(px(40.))
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(pal().line))
                    .child(div().text_sm().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(match side_panel {
                        Panel::Transcript => tr("Transcript", "转写"),
                        Panel::Library => tr("Meetings", "会议"),
                        Panel::Summary => tr("Summary", "摘要"),
                        Panel::Export => tr("Export", "导出"),
                        Panel::Import => tr("Import", "导入"),
                        Panel::Speakers => tr("Speakers", "说话人"),
                        Panel::Settings => tr("Settings", "设置"),
                    }))
                    // The title bar's button opens and closes the transcript; the other panels close back to it.
                    .when(side_panel != Panel::Transcript, |d| {
                        d.child(icon_button("close-panel", "x", tr("Back to the transcript (Esc)", "回到转写（Esc）")).on_click(cx.listener(|this, _, _, cx| {
                            this.panel = Panel::Transcript;
                            cx.notify();
                        })))
                    }),
            )
            .when(side_panel == Panel::Transcript, |d| {
                d.child(transcript).when(editable, |d| {
                    d.child(
                        div()
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .when_some(self.speaker.clone(), |d, name| {
                                let picked = name.clone();
                                d.child(
                                    div()
                                        .id("next-speaker")
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(rgb(pal().muted))
                                        .child(speaker_badge(&speakers, &name))
                                        .child(format!("{}{name}", tr("Typing as ", "输入身份：")))
                                        .on_click(cx.listener(move |this, _, window, cx| this.choose_speaker(picked.clone(), window, cx))),
                                )
                            })
                            .child(Input::new(&self.input)),
                    )
                })
            })
            .when_some(summary_panel, |d, panel| d.child(panel))
            .when_some(speakers_panel, |d, panel| d.child(panel));
        // A dialog for what is done in one go, over a dimmed window; clicking outside closes it.
        let dialog = |title: &'static str, body: gpui_kit::Stateful<gpui_kit::Div>, cx: &mut Context<Self>| {
            let mut veil: Hsla = rgb(pal().strong).into();
            veil.a = 0.18;
            div()
                .id("dialog-veil")
                .occlude()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(veil)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.panel = Panel::Transcript;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("dialog")
                        .occlude()
                        .w(px(520.))
                        .max_h(gpui_kit::relative(0.82))
                        .flex()
                        .flex_col()
                        .bg(rgb(pal().surface))
                        .border_1()
                        .border_color(rgb(pal().line))
                        .rounded(px(3.))
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .pl_5()
                                .pr_2()
                                .pt_3()
                                .pb_1()
                                .child(div().text_lg().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(title))
                                .child(icon_button("close-dialog", "x", tr("Close (Esc)", "关闭（Esc）")).on_click(cx.listener(|this, _, _, cx| {
                                    this.panel = Panel::Transcript;
                                    cx.notify();
                                }))),
                        )
                        .child(body.px_5().pb_5().pt_2()),
                )
        };
        let overlay: Option<gpui_kit::AnyElement> = match self.panel {
            Panel::Export => export_panel.map(|body| dialog(tr("Export", "导出"), body, cx).into_any_element()),
            Panel::Import => import_panel.map(|body| dialog(tr("Import", "导入"), body, cx).into_any_element()),
            Panel::Library => Some(
                div()
                    .id("library-page")
                    .occlude()
                    .absolute()
                    .size_full()
                    .bg(rgb(pal().paper))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(760.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .px_10()
                            .pt_8()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .pb_3()
                                    .child(div().text_xl().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(tr("Meetings", "会议")))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .when(self.store.is_some(), |d| d.child(Button::new("library-page-new").small().primary().label(tr("New meeting", "新建会议")).on_click(cx.listener(|this, _, _, cx| this.new_meeting(cx)))))
                                            .child(icon_button("close-library", "x", tr("Close (Esc)", "关闭（Esc）")).on_click(cx.listener(|this, _, _, cx| {
                                                this.panel = Panel::Transcript;
                                                cx.notify();
                                            }))),
                                    ),
                            )
                            .child(library),
                    )
                    .into_any_element(),
            ),
            _ => None,
        };
        let preparing_page = self.preparing.as_ref().filter(|p| p.showing()).map(|p| {
            let fraction = p.fraction();
            let (title, detail) = if p.first_time {
                (
                    tr("Getting ready for the first time", "第一次使用，正在做准备"),
                    tr("The model is compiling for this computer's graphics card. This takes about half a minute, once. Next time it opens right away.", "正在为这台电脑的显卡编译着色器，大约需要半分钟，只需这一次，之后打开就不用等了。"),
                )
            } else {
                (tr("Getting the model ready", "正在准备板书模型"), tr("This is taking longer than usual, but it is still working.", "比平时慢一些，但仍在进行中。"))
            };
            let stage = if p.done > 0 {
                tr("Almost done", "马上就好")
            } else if p.total > 0 && p.first_time {
                tr("Compiling shaders", "正在编译着色器")
            } else if p.total > 0 {
                tr("Preparing", "正在准备")
            } else {
                tr("Loading the model", "正在加载模型")
            };
            div().id("preparing").occlude().absolute().size_full().flex().items_center().justify_center().bg(rgb(pal().paper)).child(
                div()
                    .w(px(420.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_xl().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(title))
                    .child(div().text_sm().text_color(rgb(pal().muted)).child(detail))
                    .child(ProgressBar::new("preparing-progress").value(fraction * 100.0))
                    .child(div().flex().justify_between().text_xs().text_color(rgb(pal().faint)).child(stage).child(format!("{} {:.0}%", tr("about", "约"), fraction * 100.0)))
                    .child(div().text_xs().text_color(rgb(pal().faint)).child(tr("You can carry on meanwhile. What is said is kept and goes on the board once the model is ready.", "你可以先继续。这期间说的话会先记下，模型好了就上板。")))
                    .child(div().flex().child(button("preparing-dismiss", tr("Carry on", "先继续")).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(preparing) = &mut this.preparing {
                            preparing.dismissed = true;
                        }
                        cx.notify();
                    })))),
            )
        });
        // Over the board, where a message can't be missed, and never on top of the controls.
        let notices = (view_only.is_some() || banner.is_some()).then(|| {
            div()
                .absolute()
                .top(px(36.))
                .left_0()
                .right_0()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .when_some(view_only, |d, text| d.child(div().occlude().max_w(px(560.)).px_3().py_1().rounded_lg().text_sm().bg(rgb(pal().selected)).border_1().border_color(rgb(pal().line)).child(text)))
                .when_some(banner, |d, text| d.child(div().occlude().max_w(px(560.)).px_3().py_1().rounded_lg().text_sm().text_color(rgb(pal().danger)).bg(rgb(pal().danger_soft)).border_1().border_color(rgb(pal().line)).child(text)))
        });
        // The board with its corner label above and the dock attached below, and the side panel beside them.
        let canvas_area = div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(board)
            .child(div().absolute().top(px(10.)).left(px(14.)).child(coords))
            .when_some(caption, |d, text| {
                d.child(
                    div().absolute().bottom(px(16.)).left_0().right_0().flex().justify_center().child(
                        div().occlude().max_w(px(560.)).px_3().py_1().bg(rgb(pal().strong)).text_color(rgb(pal().on_strong)).text_sm().overflow_hidden().whitespace_nowrap().child(text),
                    ),
                )
            })
            .when_some(notices, |d, notices| d.child(notices));
        let main = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(canvas_area)
            .when_some(replay_bar, |d, bar| d.child(div().occlude().flex_none().border_t_1().border_color(rgb(pal().line)).bg(rgb(pal().surface)).child(bar)))
            .child(div().occlude().child(dock));
        let content = div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .child(main)
            .when(side_shown, |d| d.child(side))
            .when(self.panel == Panel::Settings, |d| d.child(settings))
            .when_some(overlay, |d, overlay| d.child(overlay))
            .when_some(preparing_page, |d, page| d.child(page));
        div()
            .id("workbench")
            .track_focus(&self.focus)
            .font_family(theme::SANS)
            .on_key_down(cx.listener(Self::on_key))
            .on_drop(cx.listener(|this, paths: &gpui_kit::ExternalPaths, _, cx| this.import_paths(paths.paths(), cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(pal().paper))
            .text_color(rgb(pal().text))
            .child(window_bar)
            .child(content)
    }
}

#[derive(Deserialize)]
struct Case {
    id: String,
    turns: Vec<Turn>,
}

#[derive(Deserialize)]
struct Turn {
    speaker: Option<String>,
    text: String,
}

/// What the slow part of starting up has to say.
enum Startup {
    Word(Vec<Inked>),
    Ready(Box<Result<Prepared>>),
}

struct Prepared {
    state: Ink,
    live: Live,
    vram_mib: u64,
    /// The palette the board was drawn in, which the system's appearance may yet change.
    dark: bool,
}

/// The window from the moment it opens: the name being written, then the app.
struct Shell {
    workbench: Option<Entity<Workbench>>,
    word: Vec<Inked>,
    started: Instant,
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().font_family(theme::SANS).bg(rgb(pal().paper)).map(|d| match &self.workbench {
            Some(workbench) => d.child(workbench.clone()).into_any_element(),
            // The bar is there from the start, so the window can be moved while the app gets ready.
            None => d.flex().flex_col().child(TitleBar::new().bg(rgb(pal().surface)).border_color(rgb(pal().line))).child(div().flex_1().min_h_0().child(crate::splash::splash(&self.word, self.started, window))).into_any_element(),
        })
    }
}

gpui_kit::actions!(boardcast, [Quit]);

/// A Mac app quits from its menu and with Cmd+Q, and either way saves first, as closing the window does.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn app_menu(cx: &mut App) {
    cx.bind_keys([gpui_kit::KeyBinding::new("cmd-q", Quit, None)]);
    cx.set_menus(vec![gpui_kit::Menu { name: "boardcast".into(), items: vec![gpui_kit::MenuItem::action(tr("Quit boardcast", "退出 boardcast"), Quit)], disabled: false }]);
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    // Before any text is made, so even the first words are in the chosen language.
    let chosen = Settings::load();
    crate::i18n::choose(&chosen.interface_language);
    theme::choose(&chosen.appearance);
    let mut root: Option<PathBuf> = None;
    let mut case = None;
    let mut wav = None;
    let mut llm = None;
    let mut exit_when_done = false;
    let mut synthetic: Option<usize> = None;
    let mut zoom_test = false;
    let mut demo: Option<String> = None;
    // For looking at a side panel without clicking to it.
    let mut start_panel = Panel::Transcript;
    let mut zoom: Option<f32> = None;
    let mut skill = false;
    let mut deepseek = false;
    let mut open: Option<String> = None;
    let mut summarize_when_done = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--case" => case = args.next(),
            "--wav" => wav = args.next().map(PathBuf::from),
            "--llm" => llm = args.next(),
            "--exit-when-done" => exit_when_done = true,
            "--synthetic" => synthetic = args.next().map(|n| n.parse()).transpose()?,
            "--zoom-test" => zoom_test = true,
            "--demo" => demo = args.next(),
            "--panel" => {
                start_panel = match args.next().as_deref() {
                    Some("library") => Panel::Library,
                    Some("summary") => Panel::Summary,
                    Some("export") => Panel::Export,
                    Some("import") => Panel::Import,
                    Some("speakers") => Panel::Speakers,
                    Some("settings") => Panel::Settings,
                    _ => Panel::Transcript,
                }
            }
            "--zoom" => zoom = args.next().map(|v| v.parse()).transpose()?,
            "--skill" => skill = true,
            "--deepseek" => deepseek = true,
            "--open" => open = args.next(),
            "--summarize" => summarize_when_done = true,
            "--root" => root = args.next().map(PathBuf::from),
            other => anyhow::bail!("unknown flag {other}"),
        }
    }
    let mut turns = vec![];
    if let Some(id) = case {
        let root = root.as_ref().context("--case needs --root <checkout>")?;
        let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(root.join("eval/cases.json"))?)?;
        let found = cases.into_iter().find(|c| c.id == id).with_context(|| format!("no case {id}"))?;
        turns = found.turns.into_iter().map(|t| Utterance { speaker: t.speaker, text: t.text, pointing: vec![] }).collect();
    }
    let data = default_dir()?;
    let models = data.join("models");
    let resources = resources::locate(&data);
    let update_data = data.clone();
    let prefix = boardcast_update::installed_prefix();
    let scripted = case_or_measurement(&turns, &wav, &demo, synthetic);
    // Looking for updates waits a moment, so it never competes with starting up.
    let check_at = (prefix.is_some() && !scripted).then(|| Instant::now() + Duration::from_secs(8));
    let binaries = resources.binaries.clone();
    let settings = Settings::load();
    let forced_backend = deepseek || llm.is_some();
    let backend = if deepseek {
        Backend::Ready(Provider::deepseek_from_env()?)
    } else if let Some(base) = llm {
        Backend::Ready(Provider::Local { base })
    } else {
        backend_from(&settings, false, &binaries, &models)
    };
    let guidance = skill.then_some(Guidance { rules: true, hint: true });
    let hanzi = resources.hanzi.clone();
    let pdfium = resources.pdfium.clone();
    let start_board = match &demo {
        Some(name) => Some(demo_board(root.as_ref().context("--demo needs --root <checkout>")?, name)?),
        None => synthetic.map(synthetic_board),
    };
    // A sample or measurement never reads or writes real meetings, and a script starts a new one.
    let store = start_board.is_none().then(|| Store::new(data.clone()));
    let reopen = if turns.is_empty() && wav.is_none() { open.or_else(|| store.as_ref().and_then(Store::last_opened)) } else { None };
    let reads_meetings = store.is_some();
    let speech = Speech { helper: Speech::helper_path(), system: settings.system_speech, language: settings.speech_language.clone() };
    let pipeline = Pipeline::start(backend, guidance, resources.voice, speech, Script { turns, wav });
    pipeline.session().set_glossary(Glossary::parse(&settings.glossary));
    std::thread::spawn(boardcast_agent::sound::warm);
    // Everything slow happens away from the window, which has to appear straight away.
    let (startup, starting) = channel::<Startup>();
    let probe_binaries = binaries.clone();
    let messy = settings.messy;
    let hanzi_source = resources.hanzi;
    std::thread::spawn(move || {
        let prepared = (|| -> Result<Prepared> {
            let dark = theme::is_dark();
            let mut glyphs = Glyphs::load(hanzi_source)?;
            glyphs.set_messy(messy);
            let _ = startup.send(Startup::Word(crate::splash::word_strokes(&mut glyphs)));
            let start_meeting = match (reads_meetings.then(|| Store::new(data)), reopen) {
                (Some(store), Some(id)) => store.load(&id).ok().flatten().map(Live::from_loaded),
                _ => None,
            };
            let vram_mib = boardcast_agent::list_gpus(&probe_binaries).iter().filter(|g| g.discrete).map(|g| g.total_mib).max().unwrap_or(0);
            let mut state = Ink {
                glyphs,
                board: StableLayout::default(),
                base: Tier::new(1.0),
                detail: None,
                retired: vec![],
                queue: VecDeque::new(),
                live: 0.0,
                finished: vec![],
                sliding: vec![],
                slide_started: Instant::now(),
                drawn: HashSet::new(),
                last: None,
                timings: vec![],
                graph: None,
                next_tidy_batch: 0,
                targets: vec![],
                ink_boxes: vec![],
                insets: [0.0; 4],
                trace: None,
                selected: None,
                selected_at: None,
                mark_unit: MarkUnit::Block,
                page: None,
                hints: None,
                extent: BoardBounds::default(),
                view: None,
                reading: false,
                glide: None,
                canvas: Bounds::new(point(px(0.), px(0.)), size(px(960.), px(756.))),
                editing: false,
                dpr: 1.0,
                zoomed_at: Instant::now(),
                touched: Instant::now(),
                generation: 0,
                rebuilding: None,
                drag: None,
                circling: false,
                loops: vec![],
                drawing: None,
                pointed: vec![],
                zoom_test: zoom_test.then_some(ZoomTest { started: None, frames: vec![], last: None, base: 0.0, rebuilds: vec![], peak_rss: 0 }),
                instant: start_board.is_some(),
            };
            let mut live = start_meeting.unwrap_or_default();
            if start_board.is_none() {
                                state.open(&live.graph, &live.memory);
                live.adopt_summary_source(&fingerprint_of(&live));
            }
            if let Some(graph) = &start_board {
                let started = Instant::now();
                let strokes = state.update(graph, 0);
                println!("{{\"synthetic_nodes\":{},\"strokes\":{strokes},\"layout_and_compose_ms\":{}}}", graph.nodes.len(), started.elapsed().as_millis());
                                live.change_board(graph.clone(), state.board.snapshot());
            }
            if let Some(k) = zoom {
                let e = &state.extent;
                state.view = Some(View { k, x: e.x0 as f32 - 20.0 / k, y: e.y0 as f32 - 20.0 / k });
                state.reading = true;
            }
            Ok(Prepared { state, live, vram_mib, dark })
        })();
        let _ = startup.send(Startup::Ready(Box::new(prepared)));
    });
    gpui_kit::application().with_assets(theme::Assets).run(move |cx: &mut App| {
        gpui_kit::init(cx);
        theme::load_fonts(cx);
        theme::apply(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(1280.), px(800.))))),
            app_id: Some("boardcast".into()),
            // On a Mac the system bar goes, and ours takes its place with the window buttons in it.
            ..if cfg!(target_os = "macos") {
                let base = TitleBar::window_options();
                WindowOptions { titlebar: base.titlebar.map(|t| gpui_kit::TitlebarOptions { title: Some("boardcast".into()), ..t }), ..base }
            } else {
                WindowOptions { titlebar: Some(gpui_kit::TitlebarOptions { title: Some("boardcast".into()), ..Default::default() }), ..Default::default() }
            }
        };
        // Closing the last window ends the app, which also stops the local model.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let build = move |prepared: Prepared, window: &mut Window, cx: &mut App| -> Entity<Workbench> {
            let Prepared { state, live, vram_mib, .. } = prepared;
            let history: Vec<Utterance> = live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect();
            pipeline.session().open(live.graph.clone(), history);
            pipeline.start_script();
            let ink = Rc::new(RefCell::new(state));
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Type a line, Enter puts it on the board", "输入一句话，回车上板")));
            let base_url = cx.new(|cx| InputState::new(window, cx).placeholder("https://api.deepseek.com").default_value(settings.base_url.clone()));
            let messy = cx.new(|_| SliderState::new().min(0.0).max(1.0).step(0.1).default_value(settings.messy));
            let glossary = cx.new(|cx| InputState::new(window, cx).placeholder("鉴权; OpenAI; API = A P I, エーピーアイ").default_value(settings.glossary.clone()));
            let model = cx.new(|cx| InputState::new(window, cx).placeholder("deepseek-flash").default_value(settings.model.clone()));
            let key = cx.new(|cx| InputState::new(window, cx).placeholder("sk-…").masked(true));
            let editor = cx.new(|cx| TextareaState::new(window, cx));
            let arxiv_input = cx.new(|cx| InputState::new(window, cx).placeholder("arxiv.org/abs/2401.12345"));
            let import_pages = cx.new(|cx| InputState::new(window, cx).placeholder(tr("All pages, or e.g. 1-5, 8", "全部页，或如 1-5, 8")));
            let speaker_name = cx.new(|cx| InputState::new(window, cx));
            let title_input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Meeting name", "会议名称")));
            let label_input = cx.new(|cx| InputState::new(window, cx));
            let library_query = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Search meetings", "搜索会议")));
            let view = cx.new(|cx| {
                cx.subscribe_in(&input, window, |this: &mut Workbench, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.submit(window, cx);
                    }
                })
                .detach();
                cx.subscribe_in(&glossary, window, |this: &mut Workbench, field, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.form.saved.glossary = field.read(cx).value().to_string();
                        this.session().set_glossary(Glossary::parse(&this.form.saved.glossary));
                        if let Err(e) = this.form.saved.save() {
                            this.form.note = Some(format!("{e:#}"));
                        }
                    }
                })
                .detach();
                cx.subscribe_in(&speaker_name, window, |this: &mut Workbench, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.rename_speaker(window, cx);
                    }
                })
                .detach();
                cx.subscribe(&messy, |this: &mut Workbench, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Change(value) = event {
                        this.choose_messy(value.end(), cx);
                    }
                })
                .detach();
                cx.subscribe_in(&title_input, window, |this: &mut Workbench, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.finish_title(window, cx);
                    }
                })
                .detach();
                cx.subscribe_in(&label_input, window, |this: &mut Workbench, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.finish_label(true, window, cx);
                    }
                })
                .detach();
                cx.subscribe_in(&library_query, window, |_: &mut Workbench, _, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
                .detach();
                // Following the system, the window turns light or dark with it.
                cx.observe_window_appearance(window, |this: &mut Workbench, _, cx| {
                    if theme::follows_system() {
                        this.restyle(theme::is_dark(), cx);
                    }
                })
                .detach();
                cx.subscribe_in(&editor, window, |this: &mut Workbench, _, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.summary_typed(cx);
                    }
                })
                .detach();
                cx.spawn(async move |this, cx| loop {
                    cx.background_executor().timer(Duration::from_millis(33)).await;
                    if this.update(cx, Workbench::pump).is_err() {
                        break;
                    }
                })
                .detach();
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                let saved_changes = live.changes;
                let remote = settings.enabled;
                if let Some(store) = &store {
                    let _ = store.remember_opened(live.id());
                }
                let mut workbench = Workbench {
                    ink,
                    pipeline,
                    input,
                    focus,
                    live,
                    store,
                    save: SaveState::Idle,
                    saving: None,
                    saved_changes,
                    seen_changes: saved_changes,
                    last_change: Instant::now(),
                    last_attempt: None,
                    panel: start_panel,
                    settings_tab: SettingsTab::Model,
                    library: vec![],
                    pinned: vec![],
                    deleted_meeting: None,
                    inputs: vec![],
                    library_query,
                    label_input,
                    form: SettingsForm { glossary, base_url, model, key, remote, messy: messy.clone(), saved: settings, note: None, probing: None },
                    summary: SummaryPanel { editor, editing: false, job: None, draft: None, cleared: None, edits: 0, note: None, current: None },
                    replay: None,
                    import: ImportState::Idle,
                    arxiv_input,
                    import_pages,
                    speaker: None,
                    speaker_chosen: None,
                    speaker_line: None,
                    speaker_name,
                    title_input,
                    editing_title: false,
                    hearing: None,
                    speech_helper: Speech::helper_path(),
                    listening_since: None,
                    clock_second: 0,
                    preparing: None,
                    transcript_open: true,
                    speaker_hovered: None,
                    transcript_scroll: gpui_kit::ScrollHandle::new(),
                    jumped_line: None,
                    search: None,
                    search_tried: false,
                    typing: None,
                    pdfium,
                    hanzi,
                    exporting: None,
                    exported: None,
                    export_dir: None,
                    summarize_when_done,
                    forced_backend,
                    models,
                    binaries,
                    download: None,
                    update: UpdateState::Idle,
                    silent_check: false,
                    update_events: None,
                    prefix,
                    check_at,
                    data: update_data,
                    vram_mib,
                    needs_model: false,
                    notice: None,
                    status: ModelStatus::Starting,
                    heard: None,
                    page_hint: None,
                    page_quiet: 0,
                    topic_hint: None,
                    topics_offered: Default::default(),
                    topic_hint_age: 0,
                    level: 0.0,
                    next_batch: 0,
                    exit_when_done,
                    close_refused: false,
                };
                workbench.show_page_image();
                // Opened on the meetings page, the list has to be read now, as opening the page would.
                if workbench.panel == Panel::Library {
                    if let Some(list) = workbench.store.as_ref().and_then(|s| s.list().ok()) {
                        workbench.library = list;
                    }
                }
                workbench
            });
            let closing = view.clone();
            window.on_window_should_close(cx, move |_, cx| closing.update(cx, |this, cx| this.closing(cx)));
            let quitting = view.clone();
            cx.on_action(move |_: &Quit, cx: &mut App| {
                if quitting.update(cx, |this, cx| this.closing(cx)) {
                    cx.quit();
                }
            });
            #[cfg(target_os = "macos")]
            app_menu(cx);
            view
        };
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| {
                cx.spawn_in(window, async move |this, cx| {
                    let mut build = Some(build);
                    let mut ready: Option<Prepared> = None;
                    loop {
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                        while let Ok(update) = starting.try_recv() {
                            match update {
                                Startup::Word(strokes) => {
                                    let _ = this.update(cx, |shell: &mut Shell, cx| {
                                        shell.word = strokes;
                                        shell.started = Instant::now();
                                        cx.notify();
                                    });
                                }
                                Startup::Ready(prepared) => match *prepared {
                                    Ok(prepared) => ready = Some(prepared),
                                    Err(e) => {
                                        eprintln!("could not start: {e:#}");
                                        std::process::exit(1);
                                    }
                                },
                            }
                        }
                        let written = this.update(cx, |shell: &mut Shell, _| shell.started.elapsed() >= crate::splash::WRITING).unwrap_or(true);
                        if ready.is_some() && (written || scripted) {
                            break;
                        }
                    }
                    let mut prepared = ready.take().expect("checked above");
                    // The board was drawn before the window knew the system's appearance.
                    if prepared.dark != theme::is_dark() {
                        prepared.state.redraw();
                    }
                    let build = build.take().expect("runs once");
                    let _ = this.update_in(cx, |shell: &mut Shell, window, cx| {
                        shell.workbench = Some(build(prepared, window, cx));
                        cx.notify();
                    });
                })
                .detach();
                Shell { workbench: None, word: vec![], started: Instant::now() }
            })
        })
        .expect("open window");
    });
    Ok(())
}

/// The numbered references a meeting's imported paper listed, kept on its first page.
fn references_of(boards: &[Graph]) -> Vec<(u32, String)> {
    boards.iter().find_map(|g| g.extra.get("references")).and_then(|r| serde_json::from_value(r.clone()).ok()).unwrap_or_default()
}

/// The pages "1-5, 8" asks for, as ranges; empty for all, `None` when it cannot be read.
fn page_ranges(spec: &str) -> Option<Vec<(u32, u32)>> {
    let mut out = vec![];
    for part in spec.split([',', '，', '、', ';', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once(['-', '–', '~', '～']) {
            Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
            None => {
                let n: u32 = part.parse().ok()?;
                (n, n)
            }
        };
        out.push((a.min(b), a.max(b)));
    }
    Some(out)
}

/// The number a page had in its document, or 1 for a lone image.
fn page_number(graph: &Graph) -> u32 {
    graph.extra.get("underlay").and_then(|u| u.get("no")).and_then(serde_json::Value::as_u64).map_or(1, |n| n as u32)
}

#[cfg(test)]
mod tests {
    use super::{encloses, page_ranges};

    #[test]
    fn pages_to_import_are_read_from_numbers_and_ranges() {
        assert_eq!(page_ranges(""), Some(vec![]));
        assert_eq!(page_ranges("1-5, 8"), Some(vec![(1, 5), (8, 8)]));
        assert_eq!(page_ranges("3，7～9、12"), Some(vec![(3, 3), (7, 9), (12, 12)]));
        assert_eq!(page_ranges("9-7"), Some(vec![(7, 9)]));
        assert_eq!(page_ranges("one to five"), None);
    }

    #[test]
    fn a_circle_holds_what_is_inside_it() {
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(encloses(&square, [5.0, 5.0]));
        assert!(!encloses(&square, [15.0, 5.0]));
        // A loose hand-drawn loop that is not closed still counts as closed.
        let loop_ = [[0.0, 0.0], [20.0, 2.0], [22.0, 20.0], [1.0, 18.0]];
        assert!(encloses(&loop_, [10.0, 10.0]));
    }
}
