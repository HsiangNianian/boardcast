//! The first sheet: a line is said, then a pen writes the headline in stroke order and circles a word.
//! The words can be circled to jump to their sheet, and an eraser wipes the sheet to write it again.

use crate::pen::{pen_draw, text_strokes, Pen};
use crate::util::{add_class, append, clear, clear_timeout, el, frame, hand, lasso_loop, now, q, qa_in, reduced, remove_class, set_attr, set_style, set_text, svg_el, timeout, wait, win, Pt};
use std::cell::Cell;
use std::rc::Rc;
use web_sys::Element;

const SAID: &str = "what if the whiteboard wrote itself?";

/// Where circling each written word leads.
fn lead(word: &str) -> Option<&'static str> {
    match word {
        "talk" => Some("#signals"),
        "board" => Some("#story"),
        "draws" => Some("#story@0.3"),
        "itself." => Some("#after"),
        _ => None,
    }
}

struct Hero {
    svg: Element,
    said: Element,
    host: Element,
    /// Bumped by every new run, so an older one stops where it is.
    run: Cell<u32>,
}

impl Hero {
    fn live(&self, me: u32) -> bool {
        self.run.get() == me
    }
}

async fn write(h: Rc<Hero>) {
    let me = h.run.get() + 1;
    h.run.set(me);
    clear(&h.svg);
    for hit in qa_in(&h.host, ".hit") {
        hit.remove();
    }
    let (w, ht) = (f64::from(h.svg.client_width()), f64::from(h.svg.client_height()));
    set_attr(&h.svg, "viewBox", format!("0 0 {w} {ht}"));

    set_text(&h.said, "");
    add_class(&h.said, "typing");
    let mut typed = String::new();
    for ch in SAID.chars() {
        if !h.live(me) {
            return;
        }
        typed.push(ch);
        set_text(&h.said, &typed);
        wait(32).await;
    }
    remove_class(&h.said, "typing");
    wait(250).await;

    // Two lines sized to the sheet, the second indented as handwriting drifts.
    let unit = text_strokes("board draws itself.", 0.0, 0.0, 100.0, 1).width / 100.0;
    let size = ((w * 0.9) / (unit + 0.5)).min(ht * 0.42).min(210.0);
    let x0 = ((w - (unit + 0.5) * size) / 2.0).max(0.0);
    let y1 = size * 0.95;
    let y2 = y1 + size * 1.05;
    let l1 = text_strokes("talk, and the", x0, y1, size, 3);
    let l2 = text_strokes("board draws itself.", x0 + size * 0.5, y2, size, 9);
    set_attr(&h.svg, "aria-label", "talk, and the board draws itself.");
    let speed = (size / 52.0).max(1.6);
    pen_draw(&h.svg, &l1.strokes, Pen { class: "w big", speed, ..Pen::default() }).await;
    if !h.live(me) {
        return;
    }
    pen_draw(&h.svg, &l2.strokes, Pen { class: "w big", speed, ..Pen::default() }).await;
    if !h.live(me) {
        return;
    }
    let it = &l2.words[2];
    let ring = lasso_loop(it.x + it.w / 2.0, y2 - size * 0.3, it.w / 2.0 + size * 0.22, size * 0.48, 21, 1.14);
    pen_draw(&h.svg, &[ring], Pen { class: "w a big", speed: speed * 1.4, lift: 0.0 }).await;
    if !h.live(me) {
        return;
    }

    // Invisible targets over the written words, for circling.
    let box_ = h.svg.get_bounding_client_rect();
    let hb = h.host.get_bounding_client_rect();
    let first = l1.words.len();
    for (i, word) in l1.words.iter().chain(&l2.words).enumerate() {
        let name = word.text.replace(',', "");
        let Some(to) = lead(&name) else { continue };
        let y = if i < first { y1 } else { y2 };
        let hit = el("span", "hit", "");
        set_attr(&hit, "data-circle", to);
        set_attr(&hit, "data-word", &name);
        set_style(&hit, "left", &format!("{}px", box_.left() - hb.left() + word.x));
        set_style(&hit, "top", &format!("{}px", box_.top() - hb.top() + y - size * 0.75));
        set_style(&hit, "width", &format!("{}px", word.w));
        set_style(&hit, "height", &format!("{}px", size * 0.8));
        append(&h.host, &hit);
    }

    // A note inviting the reader to try it, with an arrow up to the words that can be circled. Circling
    // needs a mouse or a pen, so a touch screen gets no note.
    if !fine_pointer() {
        return;
    }
    wait(300).await;
    let note_size = (size * 0.15).max(18.0);
    let width = text_strokes("drag a circle around any word", 0.0, 0.0, note_size, 5).width;
    let nx = (w - size * 3.2).min(w - width - 8.0).max(size * 0.8);
    let note = text_strokes("drag a circle around any word", nx, y2 + size * 0.62, note_size, 5);
    pen_draw(&h.svg, &note.strokes, Pen { class: "w note", speed: 1.4, lift: 12.0 }).await;
    let a: Pt = (nx - size * 0.05, y2 + size * 0.5);
    let b: Pt = (a.0 - size * 0.5, y2 + size * 0.18);
    let arrow = [hand(&[a, b], 4.0, 3), hand(&[(b.0 + 16.0, b.1 + 2.0), b], 1.0, 4), hand(&[(b.0 + 4.0, b.1 + 15.0), b], 1.0, 5)];
    pen_draw(&h.svg, &arrow, Pen { class: "w note", speed: 1.4, lift: 4.0 }).await;
}

fn fine_pointer() -> bool {
    win().match_media("(any-pointer: fine)").ok().flatten().is_some_and(|m| m.matches())
}

/// Wipe the sheet with an eraser, then write it again.
async fn again(h: Rc<Hero>) {
    if reduced() {
        return write(h).await;
    }
    h.run.set(h.run.get() + 1);
    let eraser = q("#eraser");
    let box_ = h.svg.get_bounding_client_rect();
    let hb = h.host.get_bounding_client_rect();
    set_style(&eraser, "display", "block");
    set_style(&eraser, "top", &format!("{}px", box_.top() - hb.top()));
    set_style(&eraser, "height", &format!("{}px", box_.height()));
    let clip = h.svg.query_selector("#wipe").ok().flatten().unwrap_or_else(|| {
        let clip = svg_el("clipPath", &[("id", "wipe".into())], None);
        let _ = h.svg.prepend_with_node_1(&clip);
        svg_el("rect", &[("y", "-2000".into()), ("height", "6000".into()), ("width", "10000".into())], Some(&clip));
        clip
    });
    let rect = clip.first_element_child().expect("the wipe's rectangle");
    for g in qa_in(&h.svg, ":scope > g") {
        set_attr(&g, "clip-path", "url(#wipe)");
    }
    for hit in qa_in(&h.host, ".hit") {
        hit.remove();
    }
    let t0 = now();
    loop {
        let t = frame().await;
        let k = ((t - t0) / 900.0).min(1.0);
        let x = -140.0 + (box_.width() + 280.0) * (k * k * (3.0 - 2.0 * k));
        set_style(&eraser, "left", &format!("{}px", box_.left() - hb.left() + x));
        // A slight up and down, like a hand scrubbing.
        set_style(&eraser, "transform", &format!("translateY({}px)", (k * 30.0).sin() * 6.0));
        set_attr(&rect, "x", x + 120.0);
        if k >= 1.0 {
            break;
        }
    }
    set_style(&eraser, "display", "none");
    wait(200).await;
    write(h).await;
}

pub fn init() {
    let h = Rc::new(Hero { svg: q("#ink"), said: q("#said"), host: q("#hero-writing"), run: Cell::new(0) });
    {
        let h = h.clone();
        crate::util::on(&q("#again"), "click", move |_| wasm_bindgen_futures::spawn_local(again(h.clone())));
    }
    let resizing = Rc::new(Cell::new(0));
    {
        let h = h.clone();
        crate::util::on(&win(), "resize", move |_| {
            clear_timeout(resizing.get());
            let h = h.clone();
            resizing.set(timeout(300, move || wasm_bindgen_futures::spawn_local(write(h))));
        });
    }
    wasm_bindgen_futures::spawn_local(write(h));
}
