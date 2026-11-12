//! The drawing sheet around the page: rulers on two edges, and a timeline along the bottom. The page is the
//! meeting: scrolling plays it, and the timeline shows how far in, as a recording's time, which sheet is in view
//! and where each sheet starts.


use crate::meeting::MEETING;
use crate::util::{append, clear, clamp, data, el, on_passive, q, qa, set_style, set_text, text, toggle, two, win};
use std::rc::Rc;

pub fn init() {
    let top = q("#ruler-top");
    for (i, c) in "ABCDEFGHIJKL".chars().enumerate() {
        let s = el("span", "", &c.to_string());
        set_style(&s, "left", &format!("{}%", (i as f64 + 0.5) / 12.0 * 100.0));
        append(&top, &s);
    }
    let left = q("#ruler-left");
    let labels: Vec<_> = (0..8).map(|i| {
        let s = el("span", "", &(i + 1).to_string());
        set_style(&s, "top", &format!("{}%", (f64::from(i) + 0.5) / 8.0 * 100.0));
        append(&left, &s);
        s
    }).collect();
    let number_ruler = Rc::new(move || {
        let height = f64::from(left.client_height());
        if height <= 0.0 {
            return;
        }
        let offset = (win().scroll_y().unwrap_or(0.0).max(0.0) * 8.0 / height) as usize;
        for (i, label) in labels.iter().enumerate() {
            let value = (offset + i + 1).to_string();
            if text(label) != value {
                set_text(label, &value);
            }
        }
    });
    number_ruler();
    for event in ["scroll", "resize"] {
        let update = number_ruler.clone();
        on_passive(&win(), event, move |_| update());
    }

    // One segment per sheet, as long as the sheet is tall, so the playhead crosses each in step with the page.
    let track = q("#tb-track");
    let sheets = qa("[data-sheet]");
    let segments: Vec<_> = sheets
        .iter()
        .map(|s| {
            let seg = el("span", "tb-seg", "");
            let sheet = data(s, "sheet").unwrap_or_default();
            append(&seg, &el("em", "", sheet.split(' ').next().unwrap_or("")));
            append(&track, &seg);
            seg
        })
        .collect();
    append(&track, &el("em", "tb-fin", "fin"));
    // Minutes and seconds into the recording, not the meeting's own clock, which sheet 02 shows.
    let elapsed = |sec: f64| format!("{}:{}", two((sec / 60.0).floor()), two(sec % 60.0));
    set_text(&q("#tb-total"), &elapsed(MEETING));
    let (time, head) = (q("#tb-time"), q("#tb-head"));
    let play = Rc::new(move || {
        let w = win();
        let y = w.scroll_y().unwrap_or(0.0).max(0.0);
        let page = f64::from(crate::util::doc().document_element().map_or(0, |e| e.scroll_height()));
        let view = w.inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(0.0);
        let f = if page > view { clamp(y / (page - view)) } else { 0.0 };
        set_style(&head, "left", &format!("{}%", f * 100.0));
        set_text(&time, &elapsed(f * MEETING));
        for (seg, sheet) in segments.iter().zip(&sheets) {
            let r = sheet.get_bounding_client_rect();
            set_style(seg, "flex-grow", &format!("{}", r.height().max(1.0)));
            toggle(seg, "past", r.bottom() <= view * 0.5);
        }
    });
    play();
    for event in ["scroll", "resize"] {
        let play = play.clone();
        on_passive(&win(), event, move |_| play());
    }

    let n = q("#tb-n");
    let title = q("#tb-title");
    let titleblock = q(".titleblock");
    let io = crate::util::observer(Some("-45% 0px -45% 0px"), None, move |entry, _| {
        if !entry.is_intersecting() {
            return;
        }
        let sheet = data(&entry.target(), "sheet").unwrap_or_default();
        let (no, rest) = sheet.split_once(' ').unwrap_or((&sheet, ""));
        let _ = titleblock.class_list().toggle_with_force("on-hero", no == "01");
        if text(&n) == no {
            return;
        }
        clear(&n);
        append(&n, &el("span", "flip", no));
        clear(&title);
        append(&title, &el("span", "flip", rest));
    });
    for s in qa("[data-sheet]") {
        io.observe(&s);
    }

}
