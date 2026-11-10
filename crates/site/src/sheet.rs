//! The drawing sheet around the page: rulers on two edges and a title block that follows the section in view.
//! Also the large clock at the end, which shows the reader's own time.

use crate::util::{
    append, append_text, clear, data, el, interval, q, qa, set_style, set_text, text, two,
};

pub fn init() {
    let top = q("#ruler-top");
    for (i, c) in "ABCDEFGHIJKL".chars().enumerate() {
        let s = el("span", "", &c.to_string());
        set_style(&s, "left", &format!("{}%", (i as f64 + 0.5) / 12.0 * 100.0));
        append(&top, &s);
    }
    let left = q("#ruler-left");
    for i in 0..8 {
        let s = el("span", "", &(i + 1).to_string());
        set_style(
            &s,
            "top",
            &format!("{}%", (f64::from(i) + 0.5) / 8.0 * 100.0),
        );
        append(&left, &s);
    }

    // The meeting's clock, running from when the page opened.
    let time = q("#tb-time");
    let t0 = js_sys::Date::now();
    let base = f64::from(14 * 3600 + 2 * 60 + 4);
    interval(1000, move || {
        let s = base + ((js_sys::Date::now() - t0) / 1000.0).floor();
        set_text(
            &time,
            &format!(
                "{}:{}:{}",
                two(s / 3600.0),
                two((s / 60.0) % 60.0),
                two(s % 60.0)
            ),
        );
    });

    let n = q("#tb-n");
    let title = q("#tb-title");
    let io = crate::util::observer(Some("-45% 0px -45% 0px"), None, move |entry, _| {
        if !entry.is_intersecting() {
            return;
        }
        let sheet = data(&entry.target(), "sheet").unwrap_or_default();
        let (no, rest) = sheet.split_once(' ').unwrap_or((&sheet, ""));
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

    let now = q("#now");
    let tick = move || {
        let d = js_sys::Date::new_0();
        clear(&now);
        append_text(&now, &two(f64::from(d.get_hours())));
        append(&now, &el("span", "sep", ":"));
        append_text(&now, &two(f64::from(d.get_minutes())));
    };
    tick();
    interval(5000, tick);
}
