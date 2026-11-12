//! What a meeting leaves behind, each drawn as a pictogram made of dots. Pointing at a card sends the dots
//! off and gathers them again into what that output looks like; leaving turns it back into the pictogram.

use crate::pict::icon;
use crate::util::{data, doc, every_frame, now, qa, random, reduced, win};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};

struct Dot {
    x: f64,
    y: f64,
    // Where it goes in the picture of the output.
    tx: f64,
    ty: f64,
    // How far it swings out on the way.
    sx: f64,
    sy: f64,
    // Whether it belongs to the pictogram, and to the picture.
    from: f64,
    to: f64,
    ph: f64,
}

/// What each output looks like, drawn on the same 64 grid as the pictograms.
fn preview(name: &str) -> &'static str {
    match name {
        // A summary: to-dos, two of them ticked.
        "doc" => r#"<rect class="s" x="6" y="8" width="11" height="11" stroke-width="3.5"/><path class="s" d="M8.5 13.5L11 16L15 10" stroke-width="3"/><rect class="f" x="23" y="11.5" width="35" height="4"/>
            <rect class="s" x="6" y="27" width="11" height="11" stroke-width="3.5"/><path class="s" d="M8.5 32.5L11 35L15 29" stroke-width="3"/><rect class="f" x="23" y="30.5" width="27" height="4"/>
            <rect class="s" x="6" y="46" width="11" height="11" stroke-width="3.5"/><rect class="f" x="23" y="49.5" width="31" height="4"/>"#,
        // A replay page: a browser window, a board inside, and the bar to drag.
        "replay" => r#"<rect class="s" x="4" y="6" width="56" height="52" stroke-width="3.5"/><rect class="f" x="4" y="6" width="56" height="9"/>
            <rect class="f" x="12" y="22" width="22" height="4"/><rect class="f" x="12" y="30" width="16" height="3"/><rect class="f" x="12" y="36" width="19" height="3"/>
            <path class="f" d="M40 23L52 30L40 37Z"/><rect class="f" x="10" y="48" width="44" height="2.5"/><rect class="f" x="30" y="44" width="4" height="10"/>"#,
        // A whole-meeting pdf: pages behind pages, the front one a board.
        "board" => r#"<rect class="s" x="22" y="4" width="36" height="44" stroke-width="3"/><rect class="s" x="14" y="10" width="36" height="44" stroke-width="3"/>
            <rect class="h" x="6" y="16" width="36" height="44"/><rect class="s" x="6" y="16" width="36" height="44" stroke-width="3.5"/>
            <rect class="f" x="12" y="23" width="20" height="4"/><rect class="f" x="12" y="32" width="24" height="3"/><rect class="f" x="12" y="39" width="18" height="3"/><rect class="f" x="12" y="46" width="22" height="3"/>"#,
        // Markdown: a heading, key points, and a Mermaid flow chart.
        _ => r#"<path class="s" d="M8 6V20M14 6V20M5 10H17M5 16H17" stroke-width="3"/><rect class="f" x="22" y="10" width="36" height="5"/>
            <rect class="f" x="6" y="26" width="4" height="4"/><rect class="f" x="14" y="26" width="40" height="3.5"/><rect class="f" x="6" y="34" width="4" height="4"/><rect class="f" x="14" y="34" width="30" height="3.5"/>
            <rect class="s" x="5" y="46" width="16" height="11" stroke-width="3"/><path class="s" d="M21 51.5H31M27 47.5L31 51.5L27 55.5" stroke-width="3"/><rect class="s" x="32" y="46" width="16" height="11" stroke-width="3"/><path class="s" d="M48 51.5H56" stroke-width="3"/>"#,
    }
}

const GRID: u32 = 200;
const STEP: usize = 5;

struct Card {
    ctx: CanvasRenderingContext2d,
    dots: RefCell<Vec<Dot>>,
    size: Cell<(f64, f64)>,
    scatter: Cell<f64>,
    aim: Cell<f64>,
    visible: Cell<bool>,
}

impl Card {
    fn draw(&self, t: f64) {
        let s = if reduced() { self.aim.get() } else { self.scatter.get() + (self.aim.get() - self.scatter.get()) * 0.06 };
        self.scatter.set(s);
        let e = s * s * (3.0 - 2.0 * s);
        // Out at the middle of the way, home at either end.
        let swing = (e * std::f64::consts::PI).sin();
        let (w, h) = self.size.get();
        self.ctx.clear_rect(0.0, 0.0, w, h);
        for d in self.dots.borrow().iter() {
            let alpha = d.from + (d.to - d.from) * e;
            if alpha <= 0.01 {
                continue;
            }
            let pulse = if reduced() { 1.0 } else { 0.55 + 0.45 * (t / 600.0 + d.ph).sin() };
            self.ctx.set_global_alpha(alpha * pulse);
            self.ctx.begin_path();
            let (x, y) = (d.x + (d.tx - d.x) * e + d.sx * swing, d.y + (d.ty - d.y) * e + d.sy * swing);
            let _ = self.ctx.arc(x, y, 1.7, 0.0, std::f64::consts::TAU);
            self.ctx.fill();
        }
    }

    fn recolor(&self) {
        if let Some(canvas) = self.ctx.canvas() {
            if let Ok(Some(style)) = win().get_computed_style(&canvas) {
                if let Ok(color) = style.get_property_value("color") {
                    self.ctx.set_fill_style_str(&color);
                }
            }
        }
        self.draw(now());
    }

    /// Animate while the card is on screen.
    fn run(self: &Rc<Self>) {
        let card = self.clone();
        every_frame(move |t| {
            card.draw(t);
            card.visible.get() && !reduced()
        });
    }
}

fn context(canvas: &HtmlCanvasElement) -> CanvasRenderingContext2d {
    canvas.get_context("2d").ok().flatten().expect("a 2D canvas").unchecked_into()
}

pub fn init() {
    for cv in qa(".dot-card canvas") {
        let canvas: HtmlCanvasElement = cv.clone().unchecked_into();
        let card = Rc::new(Card { ctx: context(&canvas), dots: RefCell::default(), size: Cell::new((0.0, 0.0)), scatter: Cell::new(0.0), aim: Cell::new(0.0), visible: Cell::new(false) });
        // The pictogram on the left half, the picture of the output on the right.
        let name = data(&cv, "icon").unwrap_or_default();
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 64" width="{}" height="{GRID}"><style>.f{{fill:#fff}}.s{{fill:none;stroke:#fff}}.h{{fill:#000}}.r{{fill:#fff}}</style><rect width="128" height="64" fill="#000"/>{}<g transform="translate(64 0)">{}</g></svg>"##,
            GRID * 2,
            icon(&name),
            preview(&name)
        );
        let img = HtmlImageElement::new().expect("an image");
        let onload = {
            let (img, card, canvas) = (img.clone(), card.clone(), canvas.clone());
            Closure::once_into_js(move || {
                let dpr = match win().device_pixel_ratio() {
                    d if d > 0.0 => d.min(2.0),
                    _ => 1.0,
                };
                let (w, h) = (f64::from(canvas.client_width()), f64::from(canvas.client_height()));
                card.size.set((w, h));
                canvas.set_width((w * dpr) as u32);
                canvas.set_height((h * dpr) as u32);
                let _ = card.ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
                let off: HtmlCanvasElement = doc().create_element("canvas").expect("a canvas").unchecked_into();
                off.set_width(GRID * 2);
                off.set_height(GRID);
                let o = context(&off);
                let _ = o.draw_image_with_html_image_element(&img, 0.0, 0.0);
                let Ok(pixels) = o.get_image_data(0.0, 0.0, f64::from(GRID * 2), f64::from(GRID)) else { return };
                let pixels = pixels.data();
                let (ox, oy) = (w / 2.0 - f64::from(GRID) / 2.0, h / 2.0 - f64::from(GRID) / 2.0 + 4.0);
                let grid = GRID as usize;
                let lit = |left: usize| {
                    let mut at = Vec::new();
                    for y in (0..grid).step_by(STEP) {
                        for x in (0..grid).step_by(STEP) {
                            if pixels[(y * grid * 2 + left + x) * 4] > 128 {
                                at.push((ox + x as f64, oy + y as f64));
                            }
                        }
                    }
                    // Shuffled, so each dot travels to a far place of the other shape, not its neighbour.
                    for i in (1..at.len()).rev() {
                        at.swap(i, (random() * (i + 1) as f64) as usize);
                    }
                    at
                };
                let (icon_at, preview_at) = (lit(0), lit(grid));
                if icon_at.is_empty() || preview_at.is_empty() {
                    return;
                }
                let mut dots = card.dots.borrow_mut();
                // The longer list sets the count; spare dots fade in or out on the way.
                for i in 0..icon_at.len().max(preview_at.len()) {
                    let ((x, y), (tx, ty)) = (icon_at[i % icon_at.len()], preview_at[i % preview_at.len()]);
                    let a = random() * std::f64::consts::TAU;
                    let r = 30.0 + random() * 70.0;
                    let from = if i < icon_at.len() { 1.0 } else { 0.0 };
                    let to = if i < preview_at.len() { 1.0 } else { 0.0 };
                    dots.push(Dot { x, y, tx, ty, sx: a.cos() * r, sy: a.sin() * r, from, to, ph: random() * std::f64::consts::TAU });
                }
                drop(dots);
                card.recolor();
            })
        };
        img.set_onload(Some(onload.unchecked_ref()));
        img.set_src(&format!("data:image/svg+xml;utf8,{}", String::from(js_sys::encode_uri_component(&svg))));
        let holder = cv.closest(".dot-card").ok().flatten().expect("a dot card");
        {
            let card = card.clone();
            // With less motion there is no frame loop, so the change is drawn at once.
            crate::util::on(&holder, "pointerenter", move |_| {
                card.aim.set(1.0);
                card.draw(now());
            });
        }
        {
            let card = card.clone();
            crate::util::on(&holder, "pointerleave", move |_| {
                card.aim.set(0.0);
                card.draw(now());
            });
        }
        {
            let card = card.clone();
            crate::util::on(&win(), "boardcast-theme-change", move |_| card.recolor());
        }
        let io = crate::util::observer(None, None, move |entry, _| {
            card.visible.set(entry.is_intersecting());
            if card.visible.get() {
                card.run();
            }
        });
        io.observe(&cv);
    }
}
