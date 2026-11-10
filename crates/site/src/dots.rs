//! What a meeting leaves behind, each drawn as a pictogram made of dots. Pointing at a card scatters the
//! dots; leaving gathers them again.

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
    sx: f64,
    sy: f64,
    ph: f64,
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
        let s = self.scatter.get() + (self.aim.get() - self.scatter.get()) * 0.07;
        self.scatter.set(s);
        let e = s * s * (3.0 - 2.0 * s);
        let (w, h) = self.size.get();
        self.ctx.clear_rect(0.0, 0.0, w, h);
        self.ctx.set_fill_style_str("#f2f2f2");
        for d in self.dots.borrow().iter() {
            let pulse = if reduced() { 1.0 } else { 0.55 + 0.45 * (t / 600.0 + d.ph).sin() };
            self.ctx.set_global_alpha((1.0 - s * 0.6) * pulse);
            self.ctx.begin_path();
            let _ = self.ctx.arc(d.x + d.sx * e, d.y + d.sy * e, 1.7, 0.0, std::f64::consts::TAU);
            self.ctx.fill();
        }
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
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="{GRID}" height="{GRID}"><style>.f{{fill:#fff}}.s{{fill:none;stroke:#fff}}.h{{fill:#000}}.r{{fill:#fff}}</style><rect width="64" height="64" fill="#000"/>{}</svg>"##,
            icon(&data(&cv, "icon").unwrap_or_default())
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
                off.set_width(GRID);
                off.set_height(GRID);
                let o = context(&off);
                let _ = o.draw_image_with_html_image_element(&img, 0.0, 0.0);
                let Ok(pixels) = o.get_image_data(0.0, 0.0, f64::from(GRID), f64::from(GRID)) else { return };
                let pixels = pixels.data();
                let (ox, oy) = (w / 2.0 - f64::from(GRID) / 2.0, h / 2.0 - f64::from(GRID) / 2.0 + 4.0);
                let grid = GRID as usize;
                let mut dots = card.dots.borrow_mut();
                for y in (0..grid).step_by(STEP) {
                    for x in (0..grid).step_by(STEP) {
                        if pixels[(y * grid + x) * 4] <= 128 {
                            continue;
                        }
                        let a = random() * std::f64::consts::TAU;
                        let r = 60.0 + random() * 140.0;
                        dots.push(Dot { x: ox + x as f64, y: oy + y as f64, sx: a.cos() * r, sy: a.sin() * r, ph: random() * std::f64::consts::TAU });
                    }
                }
                drop(dots);
                card.draw(now());
            })
        };
        img.set_onload(Some(onload.unchecked_ref()));
        img.set_src(&format!("data:image/svg+xml;utf8,{}", String::from(js_sys::encode_uri_component(&svg))));
        let holder = cv.closest(".dot-card").ok().flatten().expect("a dot card");
        {
            let card = card.clone();
            crate::util::on(&holder, "pointerenter", move |_| card.aim.set(1.0));
        }
        {
            let card = card.clone();
            crate::util::on(&holder, "pointerleave", move |_| card.aim.set(0.0));
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
