//! Circle anything on the page, as in the app: drag a loop around a word and the page goes to its sheet.
//! Mouse and pen only, since a finger dragging on a phone means scrolling.

use crate::util::{add_class, append, append_text, clear, clear_timeout, data, doc, el, encloses, q, qa, reduced, remove_class, set_attr, svg_el, text, timeout, to_d, win, Pt};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, PointerEvent};

const SKIP: &str = "a, button, input, kbd, .signals, .card, .dot-card, .app, .titleblock";

struct Drawing {
    pts: Vec<Pt>,
    path: Element,
}

pub fn init() {
    let layer = q("#lasso-layer");
    let toast = q("#toast");
    let drawing: Rc<RefCell<Option<Drawing>>> = Rc::default();
    let hide = Rc::new(Cell::new(0));

    let say = Rc::new(move |line: &str| {
        clear(&toast);
        append(&toast, &el("b", "", "you"));
        append_text(&toast, &format!("{} · {line}", text(&q("#tb-time"))));
        add_class(&toast, "on");
        clear_timeout(hide.get());
        let toast = toast.clone();
        hide.set(timeout(2600, move || remove_class(&toast, "on")));
    });

    let body = || doc().body().expect("a body");
    let target = win();
    {
        let drawing = drawing.clone();
        crate::util::on(&target, "pointerdown", move |e| {
            let e: PointerEvent = e.unchecked_into();
            let skipped = e.target().and_then(|t| t.dyn_into::<Element>().ok()).is_some_and(|t| t.closest(SKIP).ok().flatten().is_some());
            if e.button() != 0 || e.pointer_type() == "touch" || skipped {
                return;
            }
            e.prevent_default();
            let path = svg_el("path", &[], Some(&layer));
            *drawing.borrow_mut() = Some(Drawing { pts: vec![(f64::from(e.client_x()), f64::from(e.client_y()))], path });
            add_class(&body(), "drawing-lasso");
        });
    }
    {
        let drawing = drawing.clone();
        crate::util::on(&target, "pointermove", move |e| {
            let e: PointerEvent = e.unchecked_into();
            if let Some(d) = drawing.borrow_mut().as_mut() {
                d.pts.push((f64::from(e.client_x()), f64::from(e.client_y())));
                set_attr(&d.path, "d", to_d(&d.pts));
            }
        });
    }
    crate::util::on(&target, "pointerup", move |_| {
        let Some(Drawing { pts: shape, path: drawn }) = drawing.borrow_mut().take() else { return };
        remove_class(&body(), "drawing-lasso");
        let finish = {
            let drawn = drawn.clone();
            move || {
                add_class(&drawn, "out");
                timeout(700, move || drawn.remove());
            }
        };
        let (xs, ys): (Vec<f64>, Vec<f64>) = shape.iter().copied().unzip();
        let spread = |v: &[f64]| v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min);
        if spread(&xs) < 24.0 || spread(&ys) < 16.0 {
            return finish();
        }
        let mut closed = shape.clone();
        closed.push(shape[0]);
        set_attr(&drawn, "d", to_d(&closed));
        let hit = qa("[data-circle]").into_iter().find(|node| {
            let r = node.get_bounding_client_rect();
            encloses(&shape, (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0))
        });
        let Some(hit) = hit else {
            say("nothing circled. try a word in the headline.");
            timeout(500, finish);
            return;
        };
        let lead = data(&hit, "circle").unwrap_or_default();
        let (sel, frac) = lead.split_once('@').map_or((lead.as_str(), None), |(s, f)| (s, f.parse::<f64>().ok()));
        let target = q(sel);
        let name = data(&hit, "word").unwrap_or_else(|| text(&hit).trim().to_string());
        let sheet = data(&target, "sheet").unwrap_or_default();
        say(&format!("circled “{name}” → sheet {}", sheet.split(' ').next().unwrap_or_default()));
        timeout(650, move || {
            finish();
            let w = win();
            let height = target.dyn_ref::<HtmlElement>().map_or(0.0, |h| f64::from(h.offset_height()));
            let inner = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
            let offset = frac.map_or(-40.0, |f| (height - inner) * f);
            let y = target.get_bounding_client_rect().top() + w.scroll_y().unwrap_or(0.0) + offset;
            let options = web_sys::ScrollToOptions::new();
            options.set_top(y);
            options.set_behavior(if reduced() { web_sys::ScrollBehavior::Auto } else { web_sys::ScrollBehavior::Smooth });
            w.scroll_to_with_scroll_to_options(&options);
        });
    });
}
