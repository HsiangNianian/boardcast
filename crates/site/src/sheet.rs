//! The drawing sheet around the page: rulers on two edges and a title block that follows the section in view.

use crate::util::{append, clear, data, el, interval, on_passive, q, qa, set_style, set_text, text, two, win};
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

    // The meeting's clock, running from when the page opened.
    let time = q("#tb-time");
    let t0 = js_sys::Date::now();
    let base = f64::from(14 * 3600 + 2 * 60 + 4);
    interval(1000, move || {
        let s = base + ((js_sys::Date::now() - t0) / 1000.0).floor();
        set_text(&time, &format!("{}:{}:{}", two(s / 3600.0), two((s / 60.0) % 60.0), two(s % 60.0)));
    });

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
