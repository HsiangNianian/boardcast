//! The app, pinned while five chapters scroll past: listen, draw, correct, circle, replay. The board is
//! drawn from the scroll position, so scrolling back erases it. Until the reader scrolls, it plays itself.

use crate::meeting::{clock_at, LINES, MEETING};
use crate::util::{add_class, append, clamp, clear, el, every_frame, hand, interval, lasso_loop, now, on_passive, q, q_in, qa, reduced, remove_class, set_attr, set_style, set_text, star, svg_el, text, to_d, toggle, win, Pt};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, SvgGraphicsElement};

enum Kind {
    Stroke,
    /// A word written left to right by widening its clip to this width.
    Word(f64),
}

struct Part {
    node: Element,
    kind: Kind,
    from: f64,
    to: f64,
    fade: Option<(f64, f64)>,
}

#[derive(Clone, Copy)]
struct Item {
    x: f64,
    y: f64,
    w: f64,
}

// When each line starts typing, and when it has reached the board, in story progress.
const LINE_AT: [f64; 6] = [0.03, 0.09, 0.15, 0.41, 0.685, 0.76];
const DRAWN_AT: [f64; 6] = [0.25, 0.35, 0.4, 0.6, 0.74, 0.8];

struct Board {
    svg: Element,
    parts: Vec<Part>,
    items: HashMap<&'static str, Item>,
    clips: usize,
}

impl Board {
    fn stroke(&mut self, pts: &[Pt], class: &str, from: f64, to: f64) -> &mut Part {
        let node = svg_el("path", &[("d", to_d(pts)), ("class", format!("stroke {class}")), ("pathLength", "1".into())], Some(&self.svg));
        crate::util::set_style(&node, "stroke-dasharray", "1");
        self.parts.push(Part { node, kind: Kind::Stroke, from, to, fade: None });
        self.parts.last_mut().expect("just added")
    }

    fn word(&mut self, id: &'static str, x: f64, y: f64, s: &str, size: f64, from: f64, to: f64) {
        let cid = format!("story-clip-{}", self.clips);
        self.clips += 1;
        let clip = svg_el("clipPath", &[("id", cid.clone())], Some(&self.svg));
        let rect = svg_el("rect", &[("x", (x - 4.0).to_string()), ("y", (y - size * 1.1).to_string()), ("width", "0".into()), ("height", (size * 1.6).to_string())], Some(&clip));
        let t = svg_el("text", &[("x", x.to_string()), ("y", y.to_string()), ("class", "hand".into()), ("font-size", size.to_string()), ("clip-path", format!("url(#{cid})"))], Some(&self.svg));
        set_text(&t, s);
        let w = t.unchecked_ref::<SvgGraphicsElement>().get_b_box().map_or(0.0, |b| f64::from(b.width()));
        self.items.insert(id, Item { x, y, w });
        self.parts.push(Part { node: rect, kind: Kind::Word(w + 8.0), from, to, fade: None });
    }
}

struct Ui {
    lis: Vec<Element>,
    articles: Vec<Element>,
    tabs: Vec<Element>,
    meter: Vec<Element>,
    marks: Element,
    strike: Element,
    sel: Element,
    state: Element,
    clock: Element,
    mic: Element,
    mic_label: Element,
    lasso: Element,
    replaybar: Element,
    replay: Element,
    rfill: Element,
    rhead: Element,
    rtime: Element,
    coords: Element,
}

fn render(board: &Board, ui: &Ui, p: f64) {
    // In the last chapter the board is replayed: the scrub head drives it back through the meeting.
    let replaying = (0.8..0.985).contains(&p);
    let q = if replaying { 0.2 + 0.6 * clamp((p - 0.82) / 0.15) } else { p };
    for part in &board.parts {
        let k = clamp((q - part.from) / (part.to - part.from));
        match part.kind {
            Kind::Word(w) => set_attr(&part.node, "width", w * k),
            Kind::Stroke => {
                set_style(&part.node, "stroke-dashoffset", &(1.0 - k).to_string());
                if let Some((a, b)) = part.fade {
                    set_style(&part.node, "opacity", &(0.8 * (1.0 - clamp((q - a) / (b - a)))).to_string());
                }
            }
        }
    }
    // The transcript types each line in, newest on top.
    for (i, l) in LINES.iter().enumerate() {
        let k = clamp((q - LINE_AT[i]) / 0.04);
        let li = &ui.lis[i];
        toggle(li, "gone", k <= 0.0);
        let chars: Vec<char> = l.say.chars().collect();
        let shown: String = chars[..((chars.len() as f64 * k).ceil() as usize).min(chars.len())].iter().collect();
        let span = q_in(li, "span");
        if text(&span) != shown {
            set_text(&span, &shown);
        }
        toggle(li, "live", k > 0.0 && k < 1.0);
        toggle(li, "drawn", q >= DRAWN_AT[i]);
    }
    let ch = ((p / 0.2).floor() as usize).min(4);
    for (i, a) in ui.articles.iter().enumerate() {
        toggle(a, "on", i == ch);
    }
    for (i, t) in ui.tabs.iter().enumerate() {
        toggle(t, "on", i == ch);
        set_style(t, "--k", &clamp((p - i as f64 * 0.2) / 0.2).to_string());
    }
    let typing = LINE_AT.iter().any(|&at| q > at && q < at + 0.04);
    let drawing = board.parts.iter().any(|pt| q > pt.from && q < pt.to);
    set_text(&ui.state, if drawing { "drawing" } else if typing { "listening" } else if q > 0.02 { "saved" } else { "idle" });
    toggle(&ui.state, "hot", drawing);
    let listening = p < 0.8;
    set_text(&ui.clock, &clock_at(clamp(q / 0.8) * MEETING * 0.9));
    toggle(&ui.clock, "off", !listening);
    toggle(&ui.mic, "off", !listening);
    let word = if listening { "stop" } else { "listen" };
    if ui.mic_label.first_child().and_then(|c| c.text_content()).as_deref() != Some(word) {
        clear(&ui.mic_label);
        crate::util::append_text(&ui.mic_label, word);
        append(&ui.mic_label, &el("small", "", "space"));
    }
    let t = now();
    for (i, b) in ui.meter.iter().enumerate() {
        let height = if typing { format!("{}px", 4.0 + (t / 90.0 + i as f64 * 1.3).sin().abs() * 20.0) } else { "3px".into() };
        set_style(b, "height", &height);
    }
    // Correcting: the selection and its bar beside it.
    let correcting = !replaying && q > 0.43 && q < 0.53;
    set_style(&ui.sel, "opacity", if correcting { "1" } else { "0" });
    toggle(&ui.marks, "on", correcting);
    toggle(&ui.strike, "hit", q > 0.47);
    let i3 = board.items["i3"];
    let r = board.svg.get_bounding_client_rect();
    let scale = (r.width() / 560.0).min(r.height() / 230.0);
    set_style(&ui.marks, "left", &format!("{}px", 36.0 + (i3.x + i3.w - 6.0) * scale));
    set_style(&ui.marks, "top", &format!("{}px", 34.0 + (i3.y - 62.0) * scale));
    toggle(&ui.lasso, "active", q > 0.6 && q < 0.76);
    toggle(&ui.replaybar, "on", p >= 0.8);
    toggle(&ui.replay, "active", p >= 0.8);
    let r = clamp((p - 0.82) / 0.15);
    set_style(&ui.rfill, "width", &format!("{}%", r * 100.0));
    set_style(&ui.rhead, "left", &format!("{}%", r * 100.0));
    set_text(&ui.rtime, &clock_at(r * MEETING)[3..]);
    set_text(&ui.coords, &format!("x 0 / y 0 / 100% · {}", if replaying { "replay" } else { "live" }));
}

fn draw_board() -> Board {
    let mut b = Board { svg: q("#board"), parts: vec![], items: HashMap::new(), clips: 0 };
    b.word("t", 48.0, 70.0, "Release checklist", 32.0, 0.2, 0.25);
    let t = b.items["t"];
    b.stroke(&hand(&[(44.0, 82.0), (48.0 + t.w + 10.0, 79.0)], 3.0, 7), "ink thick", 0.25, 0.27);
    b.stroke(&hand(&[(52.0, 112.0), (62.0, 112.0)], 1.0, 2), "ink", 0.27, 0.28);
    b.word("i1", 76.0, 120.0, "Regression tests", 22.0, 0.28, 0.31);
    b.stroke(&hand(&[(52.0, 162.0), (62.0, 162.0)], 1.0, 3), "ink", 0.31, 0.32);
    b.word("i3", 76.0, 170.0, "Ship Friday", 22.0, 0.32, 0.35);
    b.stroke(&hand(&[(52.0, 212.0), (62.0, 212.0)], 1.0, 4), "ink", 0.35, 0.36);
    b.word("i2", 76.0, 220.0, "Canary 10% → 50%", 22.0, 0.36, 0.4);
    let i3 = b.items["i3"];
    b.stroke(&hand(&[(i3.x - 6.0, i3.y - 9.0), (i3.x + i3.w + 8.0, i3.y - 6.0)], 3.0, 3), "accent thick", 0.5, 0.515);
    b.stroke(&hand(&[(i3.x - 4.0, i3.y - 3.0), (i3.x + i3.w + 6.0, i3.y - 1.0)], 3.0, 5), "accent thick", 0.515, 0.53);
    let ax = i3.x + i3.w + 16.0;
    b.stroke(&hand(&[(ax, i3.y - 8.0), (ax + 100.0, i3.y - 8.0)], 2.0, 11), "ink", 0.53, 0.555);
    b.stroke(&hand(&[(ax + 90.0, i3.y - 15.0), (ax + 101.0, i3.y - 8.0), (ax + 90.0, i3.y - 1.0)], 1.0, 13), "ink", 0.555, 0.565);
    b.word("i4", ax + 116.0, 170.0, "Next Tuesday", 22.0, 0.565, 0.6);
    let i2 = b.items["i2"];
    b.stroke(&lasso_loop(i2.x + i2.w / 2.0, i2.y - 8.0, i2.w / 2.0 + 26.0, 26.0, 23, 1.12), "lasso", 0.62, 0.68).fade = Some((0.74, 0.76));
    b.stroke(&star(i2.x + i2.w + 26.0, i2.y - 8.0, 13.0), "accent", 0.7, 0.74);
    b.word("q", 380.0, 104.0, "Rollback plan?", 22.0, 0.77, 0.79);
    let qi = b.items["q"];
    let frame = [(qi.x - 14.0, qi.y - 34.0), (qi.x + qi.w + 14.0, qi.y - 32.0), (qi.x + qi.w + 12.0, qi.y + 18.0), (qi.x - 13.0, qi.y + 17.0), (qi.x - 14.0, qi.y - 34.0)];
    b.stroke(&hand(&frame, 2.5, 17), "ink", 0.79, 0.8);
    b
}

fn inner_height() -> f64 {
    win().inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn scroll_y() -> f64 {
    win().scroll_y().unwrap_or(0.0)
}

#[derive(Clone)]
struct Section(HtmlElement);

impl Section {
    fn span(&self) -> f64 {
        f64::from(self.0.offset_height()) - inner_height()
    }

    fn progress(&self) -> f64 {
        clamp(-self.0.get_bounding_client_rect().top() / self.span())
    }

    fn pinned(&self) -> bool {
        let r = self.0.get_bounding_client_rect();
        r.top() <= 1.0 && r.bottom() >= inner_height() - 1.0
    }
}

pub fn init() {
    let board = draw_board();
    let i3 = board.items["i3"];
    // The selection around "Ship Friday" while it is being corrected.
    let sel = svg_el("rect", &[("x", (i3.x - 10.0).to_string()), ("y", (i3.y - 28.0).to_string()), ("width", (i3.w + 20.0).to_string()), ("height", "40".into()), ("class", "sel".into())], Some(&board.svg));

    let list = q("#lines");
    let lis = LINES
        .iter()
        .map(|l| {
            let li = el("li", "", "");
            append(&li, &el("b", "", &format!("0{}", l.who)));
            append(&li, &el("span", "", ""));
            append(&list, &li);
            li
        })
        .collect();
    let marks = q("#marks");
    let ui = Ui {
        lis,
        articles: qa(".chapter article"),
        tabs: qa(".tabs span"),
        meter: qa("#meter i"),
        strike: q_in(&marks, ".m-strike"),
        marks,
        sel,
        state: q("#state"),
        clock: q("#clock"),
        mic: q("#mic"),
        mic_label: q("#mic-label"),
        lasso: q("#b-lasso"),
        replaybar: q("#replaybar"),
        replay: q("#b-replay"),
        rfill: q("#rfill"),
        rhead: q("#rhead"),
        rtime: q("#rtime"),
        coords: q("#coords"),
    };

    let sec = Section(q("#story").unchecked_into());
    {
        let sec = sec.clone();
        every_frame(move |_| {
            render(&board, &ui, if reduced() { 1.0 } else { sec.progress() });
            true
        });
    }
    if !reduced() {
        autoplay(sec);
    }
}

const PLAY: f64 = 4200.0;
const HOLD: f64 = 1400.0;

#[derive(Default)]
struct Auto {
    playing: Cell<bool>,
    took_over: Cell<bool>,
    settle: Cell<u32>,
}

/// Once the app has pinned and the page is still, play the chapters by scrolling at a reading pace. Any
/// wheel, touch, key or click hands the story back to the reader until they leave the section and return.
fn autoplay(sec: Section) {
    let badge = q("#auto");
    let auto = Rc::new(Auto::default());
    let stop = {
        let (auto, badge) = (auto.clone(), badge.clone());
        Rc::new(move |by_reader: bool| {
            if auto.playing.get() && by_reader {
                auto.took_over.set(true);
            }
            auto.playing.set(false);
            remove_class(&badge, "on");
        })
    };
    let play = {
        let (auto, stop, sec) = (auto.clone(), stop.clone(), sec.clone());
        move || {
            auto.playing.set(true);
            add_class(&badge, "on");
            let chapter_end = |p: f64| (((p / 0.2 + 1e-3).floor() + 1.0) * 0.2).min(1.0);
            let mut from = sec.progress();
            let mut to = chapter_end(from);
            let mut t0 = now();
            let mut holding = false;
            let (auto, stop, sec) = (auto.clone(), stop.clone(), sec.clone());
            every_frame(move |t| {
                if !auto.playing.get() {
                    return false;
                }
                if !holding {
                    let k = ((t - t0) / (PLAY * ((to - from) / 0.2).max(0.05))).min(1.0);
                    win().scroll_to_with_x_and_y(0.0, f64::from(sec.0.offset_top()) + sec.span() * (from + (to - from) * k));
                    if k >= 1.0 {
                        holding = true;
                        t0 = t;
                    }
                } else if t - t0 >= HOLD {
                    if to >= 1.0 {
                        stop(false);
                        return false;
                    }
                    holding = false;
                    from = to;
                    to = chapter_end(to);
                    t0 = t;
                }
                true
            });
        }
    };
    for ev in ["wheel", "touchstart", "keydown", "mousedown"] {
        let stop = stop.clone();
        on_passive(&win(), ev, move |_| stop(true));
    }
    let last_y = Cell::new(scroll_y());
    interval(120, move || {
        let still = scroll_y() == last_y.get();
        last_y.set(scroll_y());
        if !sec.pinned() {
            if !auto.playing.get() {
                auto.took_over.set(false);
            }
            auto.settle.set(0);
            return;
        }
        if auto.playing.get() || auto.took_over.get() || sec.progress() > 0.97 {
            return;
        }
        auto.settle.set(if still { auto.settle.get() + 1 } else { 0 });
        if auto.settle.get() >= 3 {
            play();
        }
    });
}
