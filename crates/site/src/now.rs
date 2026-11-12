//! The last sheet, the reader's own: their clock in the said line, as the meeting's was on the first sheet,
//! and a pen that writes on a blank board once it comes into view. Also marks the download for this computer.

use crate::pen::{pen_draw, text_strokes, Pen};
use crate::util::{add_class, interval, q, qa, set_attr, set_text, two, win};

fn os() -> Option<&'static str> {
    let agent = win().navigator().user_agent().ok()?.to_lowercase();
    // iPhones and iPads say "like Mac OS X", and Android says Linux; neither runs the app.
    if agent.contains("iphone") || agent.contains("ipad") || agent.contains("android") {
        None
    } else if agent.contains("mac os") {
        Some("mac")
    } else if agent.contains("windows") {
        Some("windows")
    } else if agent.contains("linux") {
        Some("linux")
    } else {
        None
    }
}

pub fn init() {
    if let Some(os) = os() {
        for a in qa(&format!(".dl [data-os=\"{os}\"]")) {
            add_class(&a, "this");
        }
    }

    let time = q("#now-time");
    let tick = move || {
        let d = js_sys::Date::new_0();
        set_text(&time, &format!("{}:{}:{}", two(f64::from(d.get_hours())), two(f64::from(d.get_minutes())), two(f64::from(d.get_seconds()))));
    };
    tick();
    interval(1000, tick);

    let svg = q("#now-ink");
    let seen = crate::util::observer(None, Some(0.6), move |entry, io| {
        if !entry.is_intersecting() {
            return;
        }
        io.disconnect();
        let svg = svg.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let (w, h) = (f64::from(svg.client_width()), f64::from(svg.client_height()));
            set_attr(&svg, "viewBox", format!("0 0 {w} {h}"));
            let unit = text_strokes("your next meeting.", 0.0, 0.0, 100.0).width / 100.0;
            let size = ((w * 0.8) / unit).min(h * 0.5).min(120.0);
            let line = text_strokes("your next meeting.", (w - unit * size) / 2.0, h / 2.0 + size * 0.3, size);
            pen_draw(&svg, &line.strokes, Pen { class: "w big", speed: (size / 52.0).max(1.4), ..Pen::default() }).await;
        });
    });
    seen.observe(&q("#now-ink"));
}
