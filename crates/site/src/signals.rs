//! One meeting as four lines on a timeline: the sound, who spoke, what went on the board, and the marks.
//! A cursor walks the meeting by itself until the reader points at it, and shows what was said then.

use crate::meeting::{clock_at, Said, LINES, MEETING};
use crate::util::{add_class, append, append_text, clamp, clear, clear_interval, el, has_class, interval, q, q_in, random, reduced, remove_class, set_style, set_text, svg_el};
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Element, MouseEvent};

fn span(l: &Said) -> (f64, f64) {
    (l.t - 4.0, l.t + 2.0 + l.say.chars().count() as f64 * 0.45)
}

fn x(sec: f64) -> f64 {
    sec / MEETING * 1000.0
}

fn flat(mut attrs: Vec<(&'static str, String)>) -> Vec<(&'static str, String)> {
    attrs.push(("vector-effect", "non-scaling-stroke".into()));
    attrs
}

pub fn init() {
    let talking = |sec: f64| LINES.iter().any(|l| sec >= span(l).0 && sec <= span(l).1);

    // Voice: a fine waveform, loud where someone speaks.
    let mut d = String::new();
    for i in (0..=1000).step_by(2) {
        let f = f64::from(i);
        let amp = if talking(f / 1000.0 * MEETING) { 18.0 + 14.0 * (f * 0.7).sin().abs() } else { 2.0 + random() * 2.0 };
        d += &format!("{}{i} {} ", if i == 0 { "M" } else { "L" }, 46.0 + (f * 1.9).sin() * amp * (random() * 0.6 + 0.4));
    }
    svg_el("path", &flat(vec![("d", d), ("class", "line draw-in".into()), ("pathLength", "1".into())]), Some(&q("#sig-voice")));

    // Who: one thin track per voice.
    let who = q("#sig-who");
    for (row, w) in ["A", "B", "C"].into_iter().enumerate() {
        let y = 22.0 + row as f64 * 24.0;
        let label = svg_el("text", &[("x", "-2".into()), ("y", (y + 3.0).to_string()), ("text-anchor", "end".into()), ("class", "pop".into())], Some(&who));
        set_text(&label, w);
        svg_el("line", &flat(vec![("x1", "0".into()), ("x2", "1000".into()), ("y1", y.to_string()), ("y2", y.to_string()), ("stroke", "var(--hair)".into())]), Some(&who));
        for l in LINES.iter().filter(|l| l.who == w) {
            let (a, b) = span(l);
            let attrs = vec![("x1", x(a).to_string()), ("x2", x(b).to_string()), ("y1", y.to_string()), ("y2", y.to_string()), ("class", "seg pop".into()), ("stroke", "var(--fg)".into())];
            svg_el("line", &flat(attrs), Some(&who));
        }
    }

    // Board: a tick each time ink went down, with what was written.
    let board = q("#sig-board");
    let written = [(34.0, "checklist"), (112.0, "regression"), (150.0, "ship friday"), (197.0, "canary 10→50%"), (342.0, "next tuesday"), (630.0, "rollback?")];
    for (i, (t, w)) in written.into_iter().enumerate() {
        svg_el("line", &flat(vec![("x1", x(t).to_string()), ("x2", x(t).to_string()), ("y1", "26".into()), ("y2", "76".into()), ("class", "tick pop".into())]), Some(&board));
        let label = svg_el("text", &[("x", (x(t) + 6.0).to_string()), ("y", (if i % 2 == 1 { "70" } else { "34" }).into()), ("class", "pop".into())], Some(&board));
        set_text(&label, w);
    }

    // Marks, made by hand, in the accent.
    let marks = q("#sig-marks");
    for (t, w, y) in [(340.0, "strike · ship friday", 38), (470.0, "circle · canary", 38), (478.0, "star", 70)] {
        let attrs = [
            ("x", (x(t) - 3.0).to_string()),
            ("y", "43".into()),
            ("width", "6".into()),
            ("height", "6".into()),
            ("class", "mark pop".into()),
            ("transform", format!("rotate(45 {} 46)", x(t))),
        ];
        svg_el("rect", &attrs, Some(&marks));
        let label = svg_el("text", &[("x", (x(t) + 9.0).to_string()), ("y", y.to_string()), ("class", "mark-label pop".into())], Some(&marks));
        set_text(&label, w);
    }

    let sig = q("#sig");
    let show = {
        let (sig, cursor, said, time) = (sig.clone(), q("#sig-cursor"), q("#sig-said"), q("#sig-time"));
        Rc::new(move |fx: f64| {
            let rows = q_in(&sig, ".row svg").get_bounding_client_rect();
            let f = clamp(fx);
            let sec = f * MEETING;
            set_style(&cursor, "left", &format!("{}px", rows.left() - sig.get_bounding_client_rect().left() + f * rows.width()));
            set_text(&q_in(&cursor, ".t"), &clock_at(sec));
            set_text(&time, &clock_at(sec));
            let last = LINES.iter().rev().find(|l| l.t - 4.0 <= sec);
            clear(&said);
            append(&said, &el("b", "", last.map_or("—", |l| l.who)));
            append_text(&said, last.map_or("nobody has spoken yet", |l| l.say));
        })
    };
    {
        let (sig2, show) = (sig.clone(), show.clone());
        crate::util::on(&sig, "pointermove", move |e| {
            let e: MouseEvent = e.unchecked_into();
            let r = q_in(&sig2, ".row svg").get_bounding_client_rect();
            remove_class(&sig2, "auto");
            show((f64::from(e.client_x()) - r.left()) / r.width());
        });
    }
    let watched: Element = sig.clone();
    let seen = crate::util::observer(None, Some(0.4), move |entry, io| {
        if !entry.is_intersecting() {
            return;
        }
        io.disconnect();
        add_class(&sig, "in");
        if reduced() {
            return;
        }
        // Until someone points, the cursor walks the meeting by itself.
        add_class(&sig, "auto");
        let (sig, show) = (sig.clone(), show.clone());
        let f = Cell::new(0.0);
        let id = Rc::new(Cell::new(0));
        let me = id.clone();
        id.set(interval(30, move || {
            if !has_class(&sig, "auto") {
                return clear_interval(me.get());
            }
            f.set((f.get() + 0.004) % 1.0);
            show(f.get());
        }));
    });
    seen.observe(&watched);
}
