//! The window: the board, the transcript, and an input that accepts Chinese through the IME.

use boardcast_board::glyphs::{Glyphs, Pt};
use boardcast_board::board_layout::{target_at, Bounds as BoardBounds, Rect, StableLayout};
use boardcast_board::check::check;
use boardcast_board::compose::{compose, Style, PAPER};
use boardcast_board::graph::{parse_ops, Graph, Utterance};
use boardcast_board::sugiyama::first_diagram;
use boardcast_agent::settings::Settings;
use boardcast_agent::{propose, Backend, Guidance, LocalServer, Provider, Said, Update};
use boardcast_meetings::{default_dir, Info, Live, Store, ViewOnly};
use boardcast_board::pen::{ink, outline, Inked, Rng};
use crate::i18n::tr;
use crate::pipeline::{Event, Pipeline, Script};
use crate::resources;
use boardcast_models::{Model, Outcome, Progress};
use boardcast_update::Release;
use crate::raster::Layer;
use anyhow::{Context as _, Result};
use boardcast_agent::summary::{self, Block, Line, Step};
use gpui_kit::base::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::{
    canvas, div, point, prelude::*, px, rgb, size, App, Background, Bounds, Context, Entity, FillOptions, FocusHandle,
    Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, PathStyle, Pixels, Point, Render, RenderImage,
    ScrollEvent, Window, WindowBounds, WindowOptions,
};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const INK_RGB: u32 = 0x1e_1e_1e;
/// What the board stores, and what each is called on its button.
const MARKS: [(&str, &str); 5] = [("划掉", "Strike"), ("圈出", "Circle"), ("下划线", "Underline"), ("星标", "Star"), ("高亮", "Highlight")];
const AUTOSAVE_AFTER: Duration = Duration::from_millis(500);
const RETRY_SAVE_AFTER: Duration = Duration::from_secs(5);
const TOOLBAR_W: f32 = 290.0;
const TOOLBAR_H: f32 = 36.0;
/// A press that moves less than this is a click, not a pan.
const CLICK_SLOP: f32 = 4.0;
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 4.0;
/// A new sharpness level is rendered once zooming has paused this long.
const RERASTER_AFTER: Duration = Duration::from_millis(120);
/// Writing speed. A backlog speeds it up so the board never falls far behind the talk.
const PEN_PX_PER_SECOND: f32 = 1500.0;

struct Queued {
    inked: Inked,
    style: Style,
    key: String,
    /// The batch this stroke belongs to, for end-to-end timing.
    batch: u64,
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
        let [_, r, g, b] = stroke.style.rgb.to_be_bytes();
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
    drawn: HashSet<String>,
    last: Option<Instant>,
    timings: Vec<Timing>,
    graph: Option<Graph>,
    next_tidy_batch: u64,
    /// What a click can land on, in board coordinates.
    targets: Vec<Rect>,
    /// The thing whose mark toolbar is open.
    selected: Option<String>,
    /// What the layout check wants the model to hear next time.
    hints: Option<String>,
    /// The content's extent, which the automatic view fits.
    extent: BoardBounds,
    /// None follows the whole board. Some keeps a view the content still fits in, or one the user chose.
    view: Option<View>,
    /// The user zoomed or panned: new content must not move the view until they return to latest.
    reading: bool,
    canvas: Bounds<Pixels>,
    dpr: f32,
    zoomed_at: Instant,
    /// Bumped when finished strokes are removed, which invalidates a rendering in progress.
    generation: u64,
    rebuilding: Option<Receiver<Rebuilt>>,
    drag: Option<(Point<Pixels>, View)>,
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

fn fill_builder() -> PathBuilder {
    PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::non_zero()))
}

fn color(style: Style) -> Hsla {
    let mut c: Hsla = rgb(style.rgb).into();
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
        if let Some(id) = &self.selected {
            if !graph.nodes.iter().any(|n| &n.id == id) {
                self.selected = None;
            }
        }
        self.follow(&layout.bounds);
        self.extent = layout.bounds;
        let wanted: HashSet<&str> = pieces.iter().map(|p| p.key.as_str()).collect();
        // Ink cannot be partly erased from a texture, so a removal or a move repaints what remains.
        if self.drawn.iter().any(|k| !wanted.contains(k.as_str())) {
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
        for piece in pieces {
            if self.drawn.contains(&piece.key) || queued.contains(&piece.key) {
                continue;
            }
            let mut r = Rng::new(&piece.seed);
            for line in &piece.lines {
                self.queue.push_back(Queued {
                    inked: ink(&line.points, line.width, line.wobble, &mut r),
                    style: line.style,
                    key: piece.key.clone(),
                    batch,
                });
                added += 1;
            }
            self.drawn.insert(piece.key);
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

    /// A click rather than a drag: select what is under the pointer, or clear the selection.
    fn click(&mut self, at: Point<Pixels>) {
        let v = self.shown();
        let x = v.x + (f32::from(at.x) - f32::from(self.canvas.origin.x)) / v.k;
        let y = v.y + (f32::from(at.y) - f32::from(self.canvas.origin.y)) / v.k;
        self.selected = target_at(&self.targets, f64::from(x), f64::from(y)).map(|t| t.id.clone());
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
        Some((x.clamp(8.0, (w - TOOLBAR_W - 8.0).max(8.0)), y.clamp(8.0, (h - TOOLBAR_H - 8.0).max(8.0))))
    }

    /// Show another meeting: nothing of the old board remains, and the new one appears at once
    /// rather than being rewritten stroke by stroke.
    fn open(&mut self, graph: &Graph, memory: &serde_json::Value) {
        let level = self.base.level;
        self.retired.extend(std::mem::replace(&mut self.base, Tier::new(level)).release());
        self.retire_detail();
        self.queue.clear();
        self.finished.clear();
        self.drawn.clear();
        self.timings.clear();
        self.targets.clear();
        self.live = 0.0;
        self.selected = None;
        self.hints = None;
        self.view = None;
        self.reading = false;
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
        let (w, h) = (f32::from(self.canvas.size.width).max(1.0), f32::from(self.canvas.size.height).max(1.0));
        let e = &self.extent;
        let bw = (e.x1 - e.x0) as f32 + 120.0;
        let bh = (e.y1 - e.y0) as f32 + 120.0;
        let k = (w / bw).min(h / bh).min(1.0);
        View { k, x: (e.x0 + e.x1) as f32 / 2.0 - w / (2.0 * k), y: (e.y0 + e.y1) as f32 / 2.0 - h / (2.0 * k) }
    }

    fn shown(&self) -> View {
        self.view.unwrap_or_else(|| self.fit())
    }

    /// New content keeps the view while it fits, so the board does not rescale with every line.
    fn follow(&mut self, next: &BoardBounds) {
        if self.reading {
            return;
        }
        let cur = self.shown();
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        let inside = self.extent.x1 > self.extent.x0
            && next.x0 as f32 >= cur.x
            && next.y0 as f32 >= cur.y
            && next.x1 as f32 <= cur.x + w / cur.k
            && next.y1 as f32 <= cur.y + h / cur.k;
        let view = inside.then_some(cur);
        if view.map(|v| v.k) != self.view.map(|v| v.k) {
            self.zoomed_at = Instant::now();
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
        self.zoomed_at = Instant::now();
    }

    fn show_latest(&mut self) {
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
        if self.zoomed_at.elapsed() < RERASTER_AFTER {
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
        if self.queue.is_empty() {
            self.instant = false;
        }
        let now = Instant::now();
        let dt = self.last.replace(now).map_or(0.016, |l| (now - l).as_secs_f32()).min(0.1);
        let mut budget = if self.instant { f32::MAX } else { PEN_PX_PER_SECOND * (1.0 + self.queue.len() as f32 / 60.0) * dt };
        while budget > 0.0 {
            let Some(front) = self.queue.front() else { break };
            let (id, length) = (front.batch, front.inked.length);
            if let Some(t) = self.timings.iter_mut().find(|t| t.id == id && t.first_ink.is_none()) {
                t.first_ink = Some(now);
            }
            let left = length * (1.0 - self.live);
            if left > budget {
                self.live += budget / length.max(0.01);
                return;
            }
            budget -= left;
            self.live = 0.0;
            let done = self.queue.pop_front().expect("front exists");
            let shape = outline(&done.inked, 1.0);
            let stroke = Arc::new(Stroke { key: done.key, area: area_of(&shape), shape, style: done.style });
            self.base.fill(&stroke);
            if let Some((tier, cover)) = &mut self.detail {
                if meets(stroke.area, *cover) {
                    tier.fill(&stroke);
                }
            }
            self.finished.push(stroke);
            if !self.queue.iter().any(|q| q.batch == done.batch) {
                self.report(done.batch, now);
            }
        }
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
    Settings,
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
    base_url: Entity<InputState>,
    model: Entity<InputState>,
    key: Entity<InputState>,
    remote: bool,
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
    library: Vec<Info>,
    form: SettingsForm,
    summary: SummaryPanel,
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
    status: String,
    level: f32,
    next_batch: u64,
    exit_when_done: bool,
    /// A failed save blocked closing once; the next attempt closes anyway.
    close_refused: bool,
}

fn backend_from(settings: &Settings, local_only: bool, binaries: &std::path::Path, models: &std::path::Path) -> Backend {
    if settings.enabled && settings.is_complete() && !local_only {
        return Backend::Ready(Provider::Remote { base_url: settings.base_url.clone(), key: settings.api_key.clone(), model: settings.model.clone() });
    }
    match boardcast_models::current(models) {
        Some(model) => Backend::ManagedLocal(Arc::new(LocalServer { binaries: binaries.to_path_buf(), model })),
        None => Backend::NoModel,
    }
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

fn line_note(line: &serde_json::Value) -> String {
    let mut parts = vec![];
    if let Some(ms) = line["heard"]["asrMs"].as_u64() {
        parts.push(format!("{} {ms} ms", tr("Heard", "识别")));
    }
    if line["failed"].as_bool().unwrap_or(false) {
        parts.push(tr("Not drawn", "没有画上").to_string());
    } else if line["ms"].is_null() {
        parts.push(tr("Drawing…", "正在画…").to_string());
    } else if let Some(ms) = line["ms"].as_u64().filter(|ms| *ms > 0) {
        parts.push(format!("{} {ms} ms", tr("Model", "模型")));
    }
    parts.join(" · ")
}

impl Workbench {
    fn session(&self) -> &boardcast_agent::Session {
        self.pipeline.session()
    }

    fn editable(&self) -> bool {
        self.live.read_only.is_none()
    }

    fn dirty(&self) -> bool {
        self.store.is_some() && self.editable() && self.live.has_content() && self.live.changes != self.saved_changes
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        {
            let mut ink = self.ink.borrow_mut();
            let before = (ink.base.level, ink.detail.as_ref().map(|(t, c)| (t.level, *c)));
            ink.maintain_sharpness();
            changed |= (ink.base.level, ink.detail.as_ref().map(|(t, c)| (t.level, *c))) != before;
            // Hints go to the model with the next line it is asked about.
            if let Some(hint) = ink.hints.take() {
                self.session().set_hint(hint);
            }
        }
        while let Ok(event) = self.pipeline.events.try_recv() {
            changed = true;
            match event {
                Event::Agent(Update::Loading) => self.status = tr("Loading the board model…", "正在加载板书模型…").into(),
                Event::Agent(Update::Ready) => {
                    self.status = tr("Board model ready", "板书模型就绪").into();
                    self.needs_model = false;
                }
                Event::Agent(Update::NoModel) => {
                    self.status = tr("Choose a board model to start", "请先选择板书模型").into();
                    self.needs_model = true;
                    self.panel = Panel::Settings;
                }
                Event::Level(l) => self.level = l,
                Event::Heard { meeting, speaker, text, asr_ms } => {
                    if meeting == self.session().meeting() {
                        self.live.add_line(speaker.as_deref(), &text, asr_ms);
                    }
                }
                Event::Agent(Update::Drawn { meeting, graph, ops, lines, asr_ms, llm_ms, ended }) => {
                    if meeting != self.session().meeting() {
                        continue;
                    }
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
                    self.live.model_drew(&spoken(&lines), graph, memory, &ops, llm_ms);
                    self.status = format!("{} {}", tr("Board", "板书"), ops.replace('\n', "  "));
                }
                Event::Agent(Update::Failed { meeting, message, lines }) => {
                    if meeting != self.session().meeting() {
                        continue;
                    }
                    eprintln!("model failed: {message}");
                    self.live.model_failed(&spoken(&lines));
                    self.notice = Some(format!("{}{message}", tr("The model did not answer: ", "模型没有回应：")));
                }
                Event::Agent(Update::Broken(message)) | Event::Failed(message) => {
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
        self.pump_probe(&mut changed);
        self.pump_summary(&mut changed, cx);
        self.pump_download(&mut changed);
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
                self.needs_model = false;
                self.form.note = Some(format!("{label} {}", tr("is ready.", "已就绪。")));
                self.apply_backend();
            }
            Ok(Outcome::Cancelled) => self.form.note = Some(tr("Paused. Download again to continue from where it stopped.", "已暂停。再次下载会从中断处继续。").into()),
            Err(error) => self.form.note = Some(error),
        }
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
        if self.pipeline.listening() {
            self.pipeline.toggle_mic();
        }
        let history: Vec<Utterance> = live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect();
        self.session().open(live.graph.clone(), history);
        self.ink.borrow_mut().open(&live.graph, &live.memory);
        self.summary.reset();
        self.live = live;
        self.live.adopt_summary_source(&fingerprint_of(&self.live));
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

    fn go_to_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.live.go_to_page(page);
        let history: Vec<Utterance> = self.live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect();
        self.session().open(self.live.graph.clone(), history);
        self.ink.borrow_mut().open(&self.live.graph, &self.live.memory);
        cx.notify();
    }

    /// Toggle a mark by hand. It is drawn at once, in the same pen as a mark from the model.
    fn mark(&mut self, id: &str, mark: &str, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let graph = self.session().toggle_mark(id, mark);
        self.next_batch += 1;
        let mut ink = self.ink.borrow_mut();
        ink.update(&graph, self.next_batch);
        let memory = ink.board.snapshot();
        drop(ink);
        self.live.change_board(graph, memory);
        cx.notify();
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
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
        // Text fields keep their own undo.
        if !self.focus.is_focused(window) {
            return;
        }
        let keys = &event.keystroke;
        if keys.modifiers.control || keys.modifiers.platform {
            match keys.key.as_str() {
                "z" if keys.modifiers.shift => self.step(true, cx),
                "z" => self.step(false, cx),
                "y" => self.step(true, cx),
                _ => {}
            }
        }
    }

    fn toggle_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.panel = if self.panel == panel { Panel::Transcript } else { panel };
        if self.panel == Panel::Library {
            if let Some(store) = &self.store {
                match store.list() {
                    Ok(list) => self.library = list,
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
        }
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
            let hello = Utterance { speaker: None, text: "Hello".into() };
            let reply = propose(&provider, &Graph::default(), &[], &hello, Guidance::default(), None);
            let _ = tx.send(reply.map(|r| r.ms).map_err(|e| format!("{e:#}")));
        });
        self.form.probing = Some(rx);
        self.form.note = Some(tr("Testing…", "正在测试…").into());
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() || !self.editable() {
            return;
        }
        self.input.update(cx, |state, cx| state.set_value("", window, cx));
        self.live.add_line(None, &text, None);
        let meeting = self.session().meeting();
        let _ = self.pipeline.said.send(Said { line: Utterance { speaker: None, text }, ended: Instant::now(), asr_ms: None, meeting });
        cx.notify();
    }

    /// On closing: save, and if that fails keep the window open once so the user can see why.
    fn closing(&mut self, cx: &mut Context<Self>) -> bool {
        if self.close_refused {
            return true;
        }
        if let Err(error) = self.flush() {
            self.close_refused = true;
            self.notice = Some(format!("{}{error}", tr("Not saved, so the window stayed open. Close again to discard: ", "没有保存成功，窗口先不关。再关一次将丢弃未保存内容：")));
            cx.notify();
            return false;
        }
        true
    }
}

/// A summary as it reads on screen. To-dos can be ticked when `tick` is given, with a target big
/// enough for a finger.
fn summary_blocks(text: &str, group: &'static str, tick: Option<&Context<Workbench>>) -> Vec<gpui_kit::AnyElement> {
    summary::blocks(text)
        .into_iter()
        .enumerate()
        .map(|(i, block)| match block {
            Block::Heading(text) => div().mt_2().text_sm().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(text).into_any_element(),
            Block::Paragraph(text) => div().text_sm().child(text).into_any_element(),
            Block::Bullet(text) => div().flex().gap_2().text_sm().child(div().flex_shrink_0().child("•")).child(div().flex_1().min_w_0().child(text)).into_any_element(),
            Block::Todo { done, text, line } => {
                let tick_box = div().flex_shrink_0().mt(px(3.)).size(px(16.)).rounded_sm().border_1().border_color(rgb(0x85_8a_80)).flex().items_center().justify_center().text_xs().text_color(rgb(0xff_ff_ff)).when(done, |d| d.bg(rgb(0x28_61_43)).border_color(rgb(0x28_61_43)).child("✓"));
                let row = div().id((group, i)).flex().items_start().gap_2().py_1().min_h(px(32.)).text_sm().child(tick_box).child(div().flex_1().min_w_0().when(done, |d| d.text_color(rgb(0x85_8a_80)).line_through()).child(text));
                match tick {
                    Some(cx) => row.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.tick_todo(line, cx))).into_any_element(),
                    None => row.into_any_element(),
                }
            }
        })
        .collect()
}

impl Workbench {
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
        let notice = |t: &'static str| div().text_xs().text_color(rgb(0x80_53_00)).child(t);
        let actions = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .when(editable, |d| match &busy {
                Some(_) => d.child(button("stop-summary", tr("Stop", "停止生成")).on_click(cx.listener(|this, _, _, cx| this.stop_summary(cx)))),
                None => d.child(
                    button("make-summary", if text.is_empty() { tr("Write summary", "生成摘要") } else { tr("Write again", "重新生成") })
                        .when(!can_make, |b| b.opacity(0.4))
                        .when(can_make, |b| b.on_click(cx.listener(|this, _, _, cx| this.make_summary(cx)))),
                ),
            })
            .when(!text.is_empty(), |d| {
                let (markdown, plain) = (text.clone(), summary::plain_text(&text));
                d.child(button("copy-markdown", tr("Copy source", "复制原文")).on_click(cx.listener(move |this, _, _, cx| this.copy_summary(markdown.clone(), tr("Copied the Markdown source", "已复制 Markdown 原文"), cx))))
                    .child(button("copy-plain", tr("Text only", "仅文本")).on_click(cx.listener(move |this, _, _, cx| this.copy_summary(plain.clone(), tr("Copied as plain text", "已复制纯文本"), cx))))
                    .when(editable, |d| {
                        d.child(button("edit-summary", if self.summary.editing { tr("Done editing", "改完了") } else { tr("Edit", "修改") }).on_click(cx.listener(|this, _, window, cx| this.edit_summary(window, cx))))
                            .child(button("clear-summary", tr("Delete", "删除")).on_click(cx.listener(|this, _, window, cx| this.clear_summary(window, cx))))
                    })
            });
        let tick = editable.then_some(&*cx);
        let reading = (!self.summary.editing).then(|| summary_blocks(&text, "todo", tick)).unwrap_or_default();
        let proposed = self.summary.draft.as_ref().map(|(draft, _)| summary_blocks(draft, "draft-todo", None));
        div()
            .id("summary-panel")
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(actions)
            .when_some(busy.clone(), |d, status| d.child(div().text_xs().text_color(rgb(0x62_66_5c)).child(status)))
            .when_some(destination.filter(|_| editable && busy.is_none() && self.summary.draft.is_none()), |d, text| d.child(div().text_xs().text_color(rgb(0x85_8a_80)).child(text)))
            .when_some(self.summary.note.clone(), |d, note| d.child(div().text_xs().text_color(rgb(0x62_66_5c)).child(note)))
            .when(stale, |d| d.child(notice(tr("The meeting has new content. Write the summary again to include it.", "会议有新内容，可重新生成摘要。"))))
            .when(self.summary.cleared.is_some(), |d| {
                d.child(div().flex().items_center().gap_2().child(div().text_xs().child(tr("Summary deleted.", "摘要已删除。"))).child(button("undo-clear", tr("Undo", "撤销删除")).on_click(cx.listener(|this, _, _, cx| this.undo_clear(cx)))))
            })
            .when(busy.is_some() && !text.is_empty(), |d| d.child(notice(tr("Your current text stays here and can be edited while the new one is written.", "正在生成新摘要，当前草稿仍可查看和编辑。"))))
            .when(text.is_empty() && !self.summary.editing && busy.is_none() && self.summary.draft.is_none() && self.summary.cleared.is_none(), |d| {
                d.child(div().text_sm().text_color(rgb(0x85_8a_80)).child(tr("Writes an editable draft from what was said: conclusions, decisions, to-dos and open questions.", "根据原话生成一份可编辑的摘要草稿：结论、决定、待办、待讨论。")))
            })
            .when(self.summary.editing, |d| d.child(div().h(px(360.)).flex_shrink_0().rounded_md().border_1().border_color(rgb(0xdd_dd_d8)).p_2().text_sm().child(Textarea::new(&self.summary.editor))))
            .children(reading)
            .when_some(proposed, |d, proposed| {
                d.child(
                    div()
                        .mt_2()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(0x28_61_43))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_sm().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(tr("New generated draft", "新生成的草稿")))
                        .child(div().text_xs().text_color(rgb(0x62_66_5c)).child(tr("Review this draft, then replace the current summary or keep your text.", "检查新草稿后，选择替换当前摘要或保留当前内容。")))
                        .when(draft_stale, |d| d.child(notice(tr("The meeting has moved on since this draft was written.", "这份草稿生成之后，会议又有了新内容。"))))
                        .children(proposed)
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_1()
                                .child(button("use-draft", tr("Replace current", "替换当前摘要")).on_click(cx.listener(|this, _, _, cx| this.use_draft(cx))))
                                .child(button("keep-current", tr("Keep current", "保留当前摘要")).on_click(cx.listener(|this, _, _, cx| {
                                    this.summary.draft = None;
                                    cx.notify();
                                }))),
                        ),
                )
            })
    }
}

fn button(id: &'static str, label: impl Into<gpui_kit::SharedString>) -> gpui_kit::Stateful<gpui_kit::Div> {
    div().id(id).px_2().py_1().rounded_md().cursor_pointer().text_sm().border_1().border_color(rgb(0xdd_dd_d8)).child(label.into())
}

fn button_id(id: (&'static str, usize), label: impl Into<gpui_kit::SharedString>) -> gpui_kit::Stateful<gpui_kit::Div> {
    div().id(id).px_2().py_1().rounded_md().cursor_pointer().text_sm().border_1().border_color(rgb(0xdd_dd_d8)).child(label.into())
}

fn case_or_measurement(turns: &[Utterance], wav: &Option<PathBuf>, demo: &Option<String>, synthetic: Option<usize>) -> bool {
    !turns.is_empty() || wav.is_some() || demo.is_some() || synthetic.is_some()
}

fn local_time(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map_or(String::new(), |t| t.format("%Y-%m-%d %H:%M").to_string())
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
    let demo: Demo = serde_json::from_slice(&std::fs::read(root.join(format!("public/demos/{name}.json")))?)?;
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
        // Only animate while there is ink to write; an idle board costs no frames.
        {
            let ink = self.ink.borrow();
            if !ink.queue.is_empty() || ink.zoom_test.is_some() {
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
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .bg(rgb(PAPER))
            .on_scroll(cx.listener(move |_, e: &ScrollEvent, window, cx| {
                let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
                // Same response as the current app's wheel: exponential in the scrolled distance.
                wheel.borrow_mut().zoom_at(e.position, (dy * 0.0015).exp());
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                window.focus(&this.focus, cx);
                let mut b = press.borrow_mut();
                let view = b.shown();
                b.drag = Some((e.position, view));
            }))
            .on_mouse_move(cx.listener(move |_, e: &MouseMoveEvent, _, cx| {
                let mut b = moved.borrow_mut();
                if let Some((from, view)) = b.drag.filter(|_| e.pressed_button == Some(MouseButton::Left)) {
                    let d = e.position - from;
                    b.view = Some(View { x: view.x - f32::from(d.x) / view.k, y: view.y - f32::from(d.y) / view.k, k: view.k });
                    b.reading = true;
                    b.zoomed_at = Instant::now();
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(move |_, e: &MouseUpEvent, _, cx| {
                let mut b = release.borrow_mut();
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
                            // The detail tier replaces the base rather than overlaying it, or
                            // the two copies of each stroke would thicken and blur together.
                            let tier = match &mut state.detail {
                                Some((tier, cover)) if inside(view, *cover) => tier,
                                _ => &mut state.base,
                            };
                            tier.under.paint(window, origin, v.k, Some(bounds));
                            tier.layer.paint(window, origin, v.k, Some(bounds));
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
        let toolbar = {
            let ink = self.ink.borrow();
            ink.toolbar_at().zip(ink.selected.clone())
        };
        if let Some(((x, y), id)) = toolbar.filter(|_| self.editable()) {
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
                    .border_color(rgb(0xdd_dd_d8))
                    .bg(rgb(0xff_fe_fa))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(MARKS.iter().map(|(mark, english)| {
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
                            .when(on, |d| d.bg(rgb(0x24_27_20)).text_color(rgb(0xff_ff_ff)))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| this.mark(&id, mark, cx)))
                    })),
            );
        }
        if reading {
            let back = self.ink.clone();
            board = board.child(
                div()
                    .id("latest")
                    .absolute()
                    .bottom(px(16.))
                    .left(px(16.))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(0x24_27_20))
                    .text_color(rgb(0xff_ff_ff))
                    .text_sm()
                    .child(tr("Back to latest", "回到最新"))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        back.borrow_mut().show_latest();
                        cx.notify();
                    })),
            );
        }
        let meter = (self.level.min(1.0) * 100.0).max(2.0);
        let editable = self.editable();
        let (page, pages) = (self.live.page(), self.live.pages());
        let save_label: String = match &self.save {
            _ if self.store.is_none() => tr("Sample, not saved", "示例，不保存").into(),
            _ if !editable => tr("View only", "只读").into(),
            SaveState::Idle => String::new(),
            SaveState::Dirty => tr("Unsaved", "未保存").into(),
            SaveState::Saving => tr("Saving…", "正在保存…").into(),
            SaveState::Saved => tr("Saved", "已保存").into(),
            SaveState::Failed(_) => tr("Not saved", "保存失败").into(),
        };
        let bar = div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(44.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(0xdd_dd_d8))
            .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).max_w(px(240.)).overflow_hidden().whitespace_nowrap().child(self.live.title().to_string()))
            .child(div().text_xs().text_color(rgb(if matches!(self.save, SaveState::Failed(_)) { 0xa3_28_26 } else { 0x62_66_5c })).whitespace_nowrap().child(save_label))
            .child(button("library", tr("Meetings", "会议")).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Library, cx))))
            .child(button("new", tr("New", "新建")).when(self.store.is_some(), |d| d.on_click(cx.listener(|this, _, _, cx| this.new_meeting(cx)))))
            .child(button("summary", tr("Summary", "摘要")).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Summary, cx))))
            .when(editable, |d| {
                d.child(
                    div()
                        .id("mic")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_sm()
                        .whitespace_nowrap()
                        .text_color(rgb(0xff_ff_ff))
                        .bg(rgb(if listening { 0xc7_44_32 } else { 0x24_27_20 }))
                        .child(if listening { tr("Stop listening", "停止收音") } else { tr("Start listening", "开始收音") })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pipeline.toggle_mic();
                            cx.notify();
                        })),
                )
                .child(div().w(px(60.)).h(px(6.)).rounded_sm().bg(rgb(0xe4_e4_df)).child(div().h_full().rounded_sm().w(px(meter * 0.6)).bg(rgb(0x28_61_43))))
            })
            .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_xs().text_color(rgb(0x62_66_5c)).child(self.status.clone()))
            .when(pages > 1, |d| {
                d.child(button("previous", "‹").on_click(cx.listener(move |this, _, _, cx| this.go_to_page(page.saturating_sub(1), cx))))
                    .child(div().text_sm().whitespace_nowrap().child(format!("{} / {pages}", page + 1)))
                    .child(button("next", "›").on_click(cx.listener(move |this, _, _, cx| this.go_to_page(page + 1, cx))))
            })
            .child(div().text_xs().text_color(rgb(0x62_66_5c)).child(format!("{:.0}%", zoom * 100.0)))
            .when(editable, |d| {
                d.child(button("undo", tr("Undo", "撤销")).when(!self.live.can_undo(), |b| b.opacity(0.4)).on_click(cx.listener(|this, _, _, cx| this.step(false, cx))))
                    .child(button("redo", tr("Redo", "重做")).when(!self.live.can_redo(), |b| b.opacity(0.4)).on_click(cx.listener(|this, _, _, cx| this.step(true, cx))))
                    .child(button("tidy", tr("Tidy", "整理排版")).on_click(cx.listener(|this, _, _, cx| this.tidy(cx))))
            })
            .when_some(match &self.update {
                UpdateState::Available(release) => Some(format!("{} {}", tr("Update to", "更新到"), release.version)),
                UpdateState::ReadyToRestart(_) => Some(tr("Restart to update", "重启完成更新").to_string()),
                _ => None,
            }, |d, label| {
                d.child(button("update-chip", label).bg(rgb(0x28_61_43)).text_color(rgb(0xff_ff_ff)).whitespace_nowrap().on_click(cx.listener(|this, _, _, cx| {
                    if matches!(this.update, UpdateState::ReadyToRestart(_)) {
                        this.restart(cx);
                    } else {
                        this.install_update(cx);
                    }
                })))
            })
            .child(button("settings", tr("Settings", "设置")).on_click(cx.listener(|this, _, _, cx| this.toggle_panel(Panel::Settings, cx))));
        let banner = self.notice.clone().or_else(|| match &self.save {
            SaveState::Failed(error) => Some(format!("{}{error}", tr("Your meeting is still open here, but it was not saved: ", "这场会还在这里，但没有保存成功："))),
            _ => None,
        });
        let view_only = self.live.read_only.map(|reason| match reason {
            ViewOnly::Unreadable => tr("This meeting's board could not be read. It is view-only and its file is unchanged.", "这场会的板书读取失败，只能查看，文件不会被改动。"),
            ViewOnly::NewerFormat => tr("This meeting was saved by a newer version, so it is view-only.", "这场会是新版本保存的，只能查看。"),
            ViewOnly::ImportedContent => tr("This meeting has imported pages or free-edit content. It is view-only here, and some of it is not shown yet. Its file is unchanged.", "这场会含导入页或自由编辑内容，这里只能查看，部分内容暂不显示，文件不会被改动。"),
        });
        let transcript = div()
            .id("transcript")
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .flex()
            .flex_col()
            .children(self.live.lines.iter().rev().map(|l| {
                let manual = l["manual"].as_bool().unwrap_or(false);
                let note = line_note(l);
                div()
                    .flex()
                    .flex_col()
                    .when_some(l["speaker"].as_str().map(str::to_string), |d, s| d.child(div().text_xs().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(s)))
                    .child(div().text_sm().when(manual, |d| d.text_color(rgb(0x85_8a_80))).child(l["text"].as_str().unwrap_or_default().to_string()))
                    .when(!note.is_empty() && !manual, |d| {
                        d.child(div().text_xs().text_color(rgb(if l["failed"].as_bool().unwrap_or(false) { 0xa3_28_26 } else { 0x85_8a_80 })).child(note))
                    })
            }));
        let library = div().id("library-list").flex_1().overflow_y_scroll().p_2().flex().flex_col().gap_1().when(self.library.is_empty(), |d| {
            d.child(div().p_2().text_sm().text_color(rgb(0x85_8a_80)).child(tr("No meetings yet.", "还没有会议。")))
        });
        let library = library.children(self.library.iter().enumerate().map(|(i, m)| {
            let id = m.id.clone();
            let current = id == self.live.id();
            let detail = match &m.read_error {
                Some(_) => tr("Could not be read", "无法读取").to_string(),
                None => format!("{} · {} {}", local_time(m.updated_at), m.lines, tr("lines", "句")),
            };
            div()
                .id(("meeting", i))
                .p_2()
                .rounded_md()
                .cursor_pointer()
                .when(current, |d| d.bg(rgb(0xec_ef_e7)))
                .child(div().text_sm().overflow_hidden().whitespace_nowrap().child(m.title.clone()))
                .child(div().text_xs().text_color(rgb(if m.read_error.is_some() { 0xa3_28_26 } else { 0x85_8a_80 })).child(detail))
                .when(m.read_error.is_none(), |d| d.on_click(cx.listener(move |this, _, _, cx| this.open_meeting(&id, cx))))
        }));
        let field = |label: &'static str, input: &Entity<InputState>| div().flex().flex_col().gap_1().child(div().text_xs().text_color(rgb(0x62_66_5c)).child(label)).child(Input::new(input));
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
            let name = format!(
                "{} · {}{}",
                model.label,
                gigabytes(model.size),
                if model.id == recommended { tr(" · recommended", " · 推荐") } else { "" },
            );
            let (action, fetch) = (model.clone(), model.clone());
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                // The name gives way, so a long one can never push its button out of reach.
                .child(div().flex_1().min_w_0().text_sm().child(name))
                .when(active, |d| d.child(div().text_xs().text_color(rgb(0x28_61_43)).child(tr("In use", "使用中"))))
                .when(installed && !active, |d| {
                    d.child(button_id(("use-model", i), tr("Use", "使用")).on_click(cx.listener(move |this, _, _, cx| this.use_model(&action, cx))))
                })
                .when(!installed && !busy, |d| {
                    d.child(button_id(("get-model", i), tr("Download", "下载")).on_click(cx.listener(move |this, _, _, cx| this.start_download(fetch.clone(), cx))))
                })
        }).collect::<Vec<_>>();
        let forced = self.forced_backend;
        let key_hint = self.form.saved.key_hint();
        let settings = div()
            .id("settings-panel")
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(tr("Board model", "板书模型")))
            .when(self.needs_model, |d| {
                d.child(div().text_sm().child(tr(
                    "To draw the board, download a model that runs on this computer, or set up a remote one. Speech recognition is already included.",
                    "要开始画板书，请下载一个在本机运行的模型，或者设置远程模型。语音识别已经内置。",
                )))
            })
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(button("use-local", tr("On this computer", "本机模型")).when(!self.form.remote, |b| b.bg(rgb(0x24_27_20)).text_color(rgb(0xff_ff_ff))).on_click(cx.listener(|this, _, _, cx| {
                        this.form.remote = false;
                        cx.notify();
                    })))
                    .child(button("use-remote", tr("Remote", "远程模型")).when(self.form.remote, |b| b.bg(rgb(0x24_27_20)).text_color(rgb(0xff_ff_ff))).on_click(cx.listener(|this, _, _, cx| {
                        this.form.remote = true;
                        cx.notify();
                    }))),
            )
            .when(!self.form.remote, |d| d.children(local_models))
            .when_some(progress_row, |d, (text, fraction)| {
                d.child(div().text_xs().child(text))
                    .child(div().h(px(6.)).w_full().rounded_sm().bg(rgb(0xe4_e4_df)).child(div().h_full().rounded_sm().w(gpui_kit::relative(fraction)).bg(rgb(0x28_61_43))))
                    .child(button("stop-download", tr("Pause", "暂停")).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(download) = &this.download {
                            download.cancel.store(true, Ordering::Relaxed);
                        }
                        cx.notify();
                    })))
            })
            .when(self.form.remote, |d| {
                d
                .child(field(tr("API address", "接口地址"), &self.form.base_url))
                .child(field(tr("Model", "模型"), &self.form.model))
                .child(field(tr("API key", "API key"), &self.form.key))
                .child(div().text_xs().text_color(rgb(0x85_8a_80)).child(match key_hint {
                    Some(hint) => format!("{} {hint}{}", tr("Saved key:", "已保存的 key："), tr(". Leave empty to keep it.", "。留空则继续使用。")),
                    None => tr("No key saved. It is stored only on this computer.", "还没有保存 key。key 只存在这台电脑上。").to_string(),
                }))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(button("save-settings", tr("Save", "保存")).on_click(cx.listener(|this, _, window, cx| this.save_settings(window, cx))))
                        .child(button("test-settings", tr("Test", "测试连接")).on_click(cx.listener(|this, _, _, cx| this.test_settings(cx)))),
                )
            })
            // Leaving the remote model takes effect on saving, like choosing it does.
            .when(!self.form.remote && self.form.saved.enabled, |d| {
                d.child(button("save-local", tr("Save", "保存")).on_click(cx.listener(|this, _, window, cx| this.save_settings(window, cx))))
            })
            .when_some(self.form.note.clone(), |d, note| d.child(div().text_sm().child(note)))
            .child(div().mt_2().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(tr("Updates", "更新")))
            .child(div().text_xs().text_color(rgb(0x62_66_5c)).child(format!("{} {}", tr("Version", "版本"), boardcast_update::VERSION)))
            .child(match &self.update {
                _ if self.prefix.is_none() => div().text_sm().child(tr("Updates apply to installed copies.", "只有安装版可以更新。")).into_any_element(),
                UpdateState::Idle => button("check-update", tr("Check for updates", "检查更新")).on_click(cx.listener(|this, _, _, cx| { this.check_for_update(); cx.notify(); })).into_any_element(),
                UpdateState::Checking => div().text_sm().child(tr("Checking…", "正在检查…")).into_any_element(),
                UpdateState::UpToDate => div().flex().items_center().gap_2().child(div().text_sm().child(tr("Up to date.", "已是最新版本。"))).child(button("check-update", tr("Check again", "再次检查")).on_click(cx.listener(|this, _, _, cx| { this.check_for_update(); cx.notify(); }))).into_any_element(),
                UpdateState::Available(release) => div().flex().flex_col().gap_2()
                    .child(div().text_sm().child(format!("{} {} · {}", tr("New version", "新版本"), release.version, gigabytes(release.size))))
                    .when(!release.notes.is_empty(), |d| d.child(div().text_xs().text_color(rgb(0x62_66_5c)).child(release.notes.clone())))
                    .child(button("install-update", tr("Download and install", "下载并安装")).on_click(cx.listener(|this, _, _, cx| this.install_update(cx))))
                    .into_any_element(),
                UpdateState::Installing { release, progress } => {
                    let text = progress.as_ref().map_or(tr("Starting…", "正在开始…").to_string(), |p| {
                        if p.done >= p.total { tr("Verifying and installing…", "正在校验并安装…").to_string() } else { format!("{} / {}", gigabytes(p.done), gigabytes(p.total)) }
                    });
                    let fraction = progress.as_ref().map_or(0.0, |p| p.done as f32 / p.total.max(1) as f32);
                    div().flex().flex_col().gap_1()
                        .child(div().text_xs().child(format!("{} {text}", release.version)))
                        .child(div().h(px(6.)).w_full().rounded_sm().bg(rgb(0xe4_e4_df)).child(div().h_full().rounded_sm().w(gpui_kit::relative(fraction)).bg(rgb(0x28_61_43))))
                        .into_any_element()
                }
                UpdateState::ReadyToRestart(release) => div().flex().flex_col().gap_2()
                    .child(div().text_sm().child(format!("{} {}", release.version, tr("is installed. Your meeting keeps running until you restart.", "已安装。重启之前，当前会议不受影响。"))))
                    .child(button("restart-update", tr("Save and restart", "保存并重启")).on_click(cx.listener(|this, _, _, cx| this.restart(cx))))
                    .into_any_element(),
                UpdateState::Failed(error) => div().flex().flex_col().gap_2()
                    .child(div().text_sm().text_color(rgb(0xa3_28_26)).child(error.clone()))
                    .child(button("check-update", tr("Try again", "重试")).on_click(cx.listener(|this, _, _, cx| { this.check_for_update(); cx.notify(); })))
                    .into_any_element(),
            })
            .when(forced, |d| d.child(div().text_xs().text_color(rgb(0x80_53_00)).child(tr("The command line chose the model, so these settings apply next launch.", "当前模型由命令行指定，这些设置下次启动生效。"))))
            .when(local_only, |d| d.child(div().text_xs().text_color(rgb(0x80_53_00)).child(tr("This meeting is local-only, so it keeps using the local model.", "这场会设置为只用本地模型，不会发送到远程。"))));
        let summary_panel = (self.panel == Panel::Summary).then(|| self.summary_panel(editable, cx));
        let side = div()
            .w(px(320.))
            .h_full()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .border_l_1()
            .border_color(rgb(0xdd_dd_d8))
            .when(self.panel == Panel::Transcript, |d| {
                d.child(transcript).when(editable, |d| d.child(div().p_3().border_t_1().border_color(rgb(0xdd_dd_d8)).child(Input::new(&self.input))))
            })
            .when(self.panel == Panel::Library, |d| d.child(library))
            .when_some(summary_panel, |d, panel| d.child(panel))
            .when(self.panel == Panel::Settings, |d| d.child(settings));
        div()
            .id("workbench")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xff_fe_fa))
            .text_color(rgb(INK_RGB))
            .child(bar)
            .when_some(view_only, |d, text| d.child(div().px_3().py_1().flex_shrink_0().text_sm().bg(rgb(0xec_ef_e7)).child(text)))
            .when_some(banner, |d, text| d.child(div().px_3().py_1().flex_shrink_0().text_sm().text_color(rgb(0xa3_28_26)).bg(rgb(0xfb_ec_ea)).child(text)))
            .child(div().flex_1().flex().min_h_0().child(board).child(side))
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

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let mut root: Option<PathBuf> = None;
    let mut case = None;
    let mut wav = None;
    let mut llm = None;
    let mut exit_when_done = false;
    let mut synthetic: Option<usize> = None;
    let mut zoom_test = false;
    let mut demo: Option<String> = None;
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
        turns = found.turns.into_iter().map(|t| Utterance { speaker: t.speaker, text: t.text }).collect();
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
    let glyphs = Glyphs::load(resources.hanzi)?;
    let start_board = match &demo {
        Some(name) => Some(demo_board(root.as_ref().context("--demo needs --root <checkout>")?, name)?),
        None => synthetic.map(synthetic_board),
    };
    // A sample or measurement never reads or writes real meetings, and a script starts a new one.
    let store = start_board.is_none().then(|| Store::new(data));
    let reopen = if turns.is_empty() && wav.is_none() { open.or_else(|| store.as_ref().and_then(Store::last_opened)) } else { None };
    let start_meeting = match (&store, reopen) {
        (Some(store), Some(id)) => store.load(&id).ok().flatten().map(Live::from_loaded),
        _ => None,
    };
    let pipeline = Pipeline::start(backend, guidance, resources.voice, Script { turns, wav });
    let vram_mib = boardcast_agent::list_gpus(&binaries).iter().filter(|g| g.discrete).map(|g| g.total_mib).max().unwrap_or(0);
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(1280.), px(800.))))),
            titlebar: Some(gpui_kit::TitlebarOptions { title: Some("boardcast".into()), ..Default::default() }),
            app_id: Some("boardcast".into()),
            ..Default::default()
        };
        let mut state = Ink {
            glyphs,
            board: StableLayout::default(),
            base: Tier::new(1.0),
            detail: None,
            retired: vec![],
            queue: VecDeque::new(),
            live: 0.0,
            finished: vec![],
            drawn: HashSet::new(),
            last: None,
            timings: vec![],
            graph: None,
            next_tidy_batch: 0,
            targets: vec![],
            selected: None,
            hints: None,
            extent: BoardBounds::default(),
            view: None,
            reading: false,
            canvas: Bounds::new(point(px(0.), px(0.)), size(px(960.), px(756.))),
            dpr: 1.0,
            zoomed_at: Instant::now(),
            generation: 0,
            rebuilding: None,
            drag: None,
            zoom_test: zoom_test.then_some(ZoomTest { started: None, frames: vec![], last: None, base: 0.0, rebuilds: vec![], peak_rss: 0 }),
            instant: start_board.is_some(),
        };
        let mut live = start_meeting.unwrap_or_default();
        if start_board.is_none() {
            pipeline.session().open(live.graph.clone(), live.history.iter().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect());
            state.open(&live.graph, &live.memory);
            live.adopt_summary_source(&fingerprint_of(&live));
        }
        if let Some(graph) = &start_board {
            let started = Instant::now();
            let strokes = state.update(graph, 0);
            println!("{{\"synthetic_nodes\":{},\"strokes\":{strokes},\"layout_and_compose_ms\":{}}}", graph.nodes.len(), started.elapsed().as_millis());
            pipeline.session().open(graph.clone(), vec![]);
            live.change_board(graph.clone(), state.board.snapshot());
        }
        if let Some(k) = zoom {
            let e = &state.extent;
            state.view = Some(View { k, x: e.x0 as f32 - 20.0 / k, y: e.y0 as f32 - 20.0 / k });
            state.reading = true;
        }
        pipeline.start_script();
        let ink = Rc::new(RefCell::new(state));
        // Closing the last window ends the app, which also stops the local model.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.open_window(options, move |window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Type a line, Enter puts it on the board", "输入一句话，回车上板")));
            let base_url = cx.new(|cx| InputState::new(window, cx).placeholder("https://api.deepseek.com").default_value(settings.base_url.clone()));
            let model = cx.new(|cx| InputState::new(window, cx).placeholder("deepseek-flash").default_value(settings.model.clone()));
            let key = cx.new(|cx| InputState::new(window, cx).placeholder("sk-…").masked(true));
            let editor = cx.new(|cx| TextareaState::new(window, cx));
            let view = cx.new(|cx| {
                cx.subscribe_in(&input, window, |this: &mut Workbench, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.submit(window, cx);
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
                Workbench {
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
                    panel: Panel::Transcript,
                    library: vec![],
                    form: SettingsForm { base_url, model, key, remote, saved: settings, note: None, probing: None },
                    summary: SummaryPanel { editor, editing: false, job: None, draft: None, cleared: None, edits: 0, note: None, current: None },
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
                    status: tr("Starting…", "启动中…").into(),
                    level: 0.0,
                    next_batch: 0,
                    exit_when_done,
                    close_refused: false,
                }
            });
            let closing = view.clone();
            window.on_window_should_close(cx, move |_, cx| closing.update(cx, |this, cx| this.closing(cx)));
            cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
        })
        .expect("open window");
    });
    Ok(())
}
