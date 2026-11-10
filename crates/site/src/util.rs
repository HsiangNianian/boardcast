//! Small helpers shared by the page: the document, timers and frames as futures, motion preferences,
//! easing, and hand-drawn lines as point lists.

use std::cell::OnceCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    CssStyleDeclaration, Document, Element, Event, HtmlElement, Node, SvgElement, Window,
};

pub type Pt = (f64, f64);

pub fn win() -> Window {
    web_sys::window().expect("a window")
}

pub fn doc() -> Document {
    win().document().expect("a document")
}

pub fn now() -> f64 {
    win().performance().map_or(0.0, |p| p.now())
}

thread_local! {
    static REDUCED: OnceCell<bool> = const { OnceCell::new() };
}

/// Whether the reader asked for less motion.
pub fn reduced() -> bool {
    REDUCED.with(|r| {
        *r.get_or_init(|| {
            win()
                .match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
                .is_some_and(|m| m.matches())
        })
    })
}

pub fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// The first element matching `sel`; the page's own markup always has it.
pub fn q(sel: &str) -> Element {
    doc()
        .query_selector(sel)
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("no {sel} on the page"))
}

pub fn q_in(root: &Element, sel: &str) -> Element {
    root.query_selector(sel)
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("no {sel} on the page"))
}

pub fn qa(sel: &str) -> Vec<Element> {
    list(doc().query_selector_all(sel).ok())
}

pub fn qa_in(root: &Element, sel: &str) -> Vec<Element> {
    list(root.query_selector_all(sel).ok())
}

fn list(nodes: Option<web_sys::NodeList>) -> Vec<Element> {
    let Some(nodes) = nodes else { return vec![] };
    (0..nodes.length())
        .filter_map(|i| nodes.item(i)?.dyn_into::<Element>().ok())
        .collect()
}

/// The inline style of an HTML or SVG element.
pub fn style(e: &Element) -> CssStyleDeclaration {
    if let Some(h) = e.dyn_ref::<HtmlElement>() {
        h.style()
    } else {
        e.unchecked_ref::<SvgElement>().style()
    }
}

pub fn set_style(e: &Element, name: &str, value: &str) {
    let _ = style(e).set_property(name, value);
}

pub fn data(e: &Element, key: &str) -> Option<String> {
    e.get_attribute(&format!("data-{key}"))
}

pub fn toggle(e: &Element, class: &str, on: bool) {
    let _ = e.class_list().toggle_with_force(class, on);
}

pub fn add_class(e: &Element, class: &str) {
    let _ = e.class_list().add_1(class);
}

pub fn remove_class(e: &Element, class: &str) {
    let _ = e.class_list().remove_1(class);
}

pub fn has_class(e: &Element, class: &str) -> bool {
    e.class_list().contains(class)
}

pub fn set_attr(e: &Element, name: &str, value: impl ToString) {
    let _ = e.set_attribute(name, &value.to_string());
}

pub fn text(e: &Element) -> String {
    e.text_content().unwrap_or_default()
}

pub fn set_text(e: &Element, s: &str) {
    e.set_text_content(Some(s));
}

pub fn clear(e: &Element) {
    e.set_text_content(None);
}

pub fn append(parent: &Element, child: &Node) {
    let _ = parent.append_child(child);
}

pub fn append_text(parent: &Element, s: &str) {
    append(parent, &doc().create_text_node(s));
}

const NS: &str = "http://www.w3.org/2000/svg";

/// An SVG element with its attributes, added to `parent` when given.
pub fn svg_el(name: &str, attrs: &[(&str, String)], parent: Option<&Element>) -> Element {
    let e = doc()
        .create_element_ns(Some(NS), name)
        .expect("an SVG element");
    for (k, v) in attrs {
        let _ = e.set_attribute(k, v);
    }
    if let Some(p) = parent {
        append(p, &e);
    }
    e
}

/// An HTML element with a class and some text, built as nodes rather than parsed from markup.
pub fn el(tag: &str, class: &str, content: &str) -> Element {
    let e = doc().create_element(tag).expect("an element");
    if !class.is_empty() {
        e.set_class_name(class);
    }
    if !content.is_empty() {
        append_text(&e, content);
    }
    e
}

/// The page's own constant SVG markup, such as a pictogram, parsed as XML into nodes for this document.
pub fn svg_parts(markup: &str) -> Vec<Node> {
    let parser = web_sys::DomParser::new().expect("a DOM parser");
    let parsed = parser
        .parse_from_string(
            &format!("<svg xmlns=\"{NS}\">{markup}</svg>"),
            web_sys::SupportedType::ImageSvgXml,
        )
        .expect("valid SVG");
    let root = parsed.document_element().expect("an SVG root");
    let children = root.child_nodes();
    (0..children.length())
        .filter_map(|i| children.item(i))
        .filter_map(|n| doc().import_node_with_deep(&n, true).ok())
        .collect()
}

/// Keep a closure alive for as long as the page is open.
pub fn on(target: &web_sys::EventTarget, event: &str, f: impl FnMut(Event) + 'static) {
    let f = Closure::<dyn FnMut(Event)>::new(f);
    let _ = target.add_event_listener_with_callback(event, f.as_ref().unchecked_ref());
    f.forget();
}

pub fn on_passive(target: &web_sys::EventTarget, event: &str, f: impl FnMut(Event) + 'static) {
    let f = Closure::<dyn FnMut(Event)>::new(f);
    let options = web_sys::AddEventListenerOptions::new();
    options.set_passive(true);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        event,
        f.as_ref().unchecked_ref(),
        &options,
    );
    f.forget();
}

pub fn interval(ms: i32, f: impl FnMut() + 'static) -> i32 {
    let f = Closure::<dyn FnMut()>::new(f);
    let id = win()
        .set_interval_with_callback_and_timeout_and_arguments_0(f.as_ref().unchecked_ref(), ms)
        .unwrap_or(0);
    f.forget();
    id
}

pub fn timeout(ms: i32, f: impl FnOnce() + 'static) -> i32 {
    let f = Closure::once_into_js(f);
    win()
        .set_timeout_with_callback_and_timeout_and_arguments_0(f.unchecked_ref(), ms)
        .unwrap_or(0)
}

pub fn clear_timeout(id: i32) {
    win().clear_timeout_with_handle(id);
}

pub fn clear_interval(id: i32) {
    win().clear_interval_with_handle(id);
}

/// Resolves after `ms`, or at once when the reader asked for less motion.
pub async fn wait(ms: i32) {
    let ms = if reduced() { 0 } else { ms };
    let promise = js_sys::Promise::new(&mut |done, _| {
        let _ = win().set_timeout_with_callback_and_timeout_and_arguments_0(&done, ms);
    });
    let _ = JsFuture::from(promise).await;
}

/// Resolves on the next animation frame, with its time.
pub async fn frame() -> f64 {
    let promise = js_sys::Promise::new(&mut |done, _| {
        let _ = win().request_animation_frame(&done);
    });
    JsFuture::from(promise)
        .await
        .ok()
        .and_then(|t| t.as_f64())
        .unwrap_or_else(now)
}

/// Run `f` on every animation frame for as long as it returns true.
pub fn every_frame(mut f: impl FnMut(f64) -> bool + 'static) {
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            let t = frame().await;
            if !f(t) {
                break;
            }
        }
    });
}

/// Watch elements come into view.
pub fn observer(
    margin: Option<&str>,
    threshold: Option<f64>,
    mut f: impl FnMut(web_sys::IntersectionObserverEntry, &web_sys::IntersectionObserver) + 'static,
) -> web_sys::IntersectionObserver {
    let callback = Closure::<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>::new(
        move |entries: js_sys::Array, io: web_sys::IntersectionObserver| {
            for entry in entries.iter() {
                f(entry.unchecked_into(), &io);
            }
        },
    );
    let options = web_sys::IntersectionObserverInit::new();
    if let Some(m) = margin {
        options.set_root_margin(m);
    }
    if let Some(t) = threshold {
        options.set_threshold(&JsValue::from_f64(t));
    }
    let io = web_sys::IntersectionObserver::new_with_options(
        callback.as_ref().unchecked_ref(),
        &options,
    )
    .expect("an observer");
    callback.forget();
    io
}

pub fn random() -> f64 {
    js_sys::Math::random()
}

/// The same small generator as the page always used, so the drawings keep their shapes.
pub struct Rand(f64);

impl Rand {
    pub fn new(seed: u32) -> Self {
        let s = f64::from(seed) % 2_147_483_647.0;
        Self(if s == 0.0 { 7.0 } else { s })
    }

    pub fn next(&mut self) -> f64 {
        self.0 = (self.0 * 16807.0) % 2_147_483_647.0;
        self.0 / 2_147_483_647.0
    }
}

pub fn to_d(pts: &[Pt]) -> String {
    let parts: Vec<String> = pts
        .iter()
        .map(|p| format!("{} {}", fixed(p.0), fixed(p.1)))
        .collect();
    format!("M{}", parts.join(" L"))
}

/// One decimal, written as JavaScript's toFixed writes it.
pub fn fixed(x: f64) -> String {
    let s = format!("{x:.1}");
    if s == "-0.0" {
        "0.0".into()
    } else {
        s
    }
}

pub fn length_of(pts: &[Pt]) -> f64 {
    pts.windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum()
}

/// A pen line through the points, with the slight wobble of a hand.
pub fn hand(points: &[Pt], wobble: f64, seed: u32) -> Vec<Pt> {
    let mut r = Rand::new(seed);
    let mut out = vec![];
    for (i, w) in points.windows(2).enumerate() {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        let len = (x1 - x0).hypot(y1 - y0);
        let n = (len / 14.0).round().max(2.0) as usize;
        let d = if len == 0.0 { 1.0 } else { len };
        let (nx, ny) = (-(y1 - y0) / d, (x1 - x0) / d);
        for k in (if i == 0 { 0 } else { 1 })..=n {
            let t = k as f64 / n as f64;
            let j = (r.next() - 0.5) * wobble * (std::f64::consts::PI * t).sin();
            out.push((x0 + (x1 - x0) * t + nx * j, y0 + (y1 - y0) * t + ny * j));
        }
    }
    out
}

/// A loop drawn around something, a little more than once round, as a hand circles.
pub fn lasso_loop(cx: f64, cy: f64, rx: f64, ry: f64, seed: u32, turns: f64) -> Vec<Pt> {
    let mut r = Rand::new(seed);
    let n = 50.0;
    let mut out = vec![];
    let mut i = 0.0;
    while i <= n * turns {
        let a = -2.4 + (i / n) * std::f64::consts::TAU;
        let k = 1.0 + (r.next() - 0.5) * 0.06 + (i / n) * 0.05;
        out.push((cx + a.cos() * rx * k, cy + a.sin() * ry * k));
        i += 1.0;
    }
    out
}

pub fn star(cx: f64, cy: f64, radius: f64) -> Vec<Pt> {
    (0..=5)
        .map(|i| {
            let a =
                -std::f64::consts::FRAC_PI_2 + (f64::from(i) * 4.0 * std::f64::consts::PI) / 5.0;
            (cx + a.cos() * radius, cy + a.sin() * radius)
        })
        .collect()
}

/// Whether a closed shape holds a point, by counting crossings.
pub fn encloses(shape: &[Pt], p: Pt) -> bool {
    let mut inside = false;
    let mut j = shape.len().wrapping_sub(1);
    for i in 0..shape.len() {
        let (a, b) = (shape[i], shape[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < ((b.0 - a.0) * (p.1 - a.1)) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Two digits, as a clock shows them.
pub fn two(n: f64) -> String {
    format!("{:02}", n.floor() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generator_matches_the_old_page() {
        let mut r = Rand::new(3);
        // 3 * 16807 = 50421, then 50421 * 16807 = 847425747.
        assert!((r.next() - 50421.0 / 2_147_483_647.0).abs() < 1e-15);
        assert!((r.next() - 847_425_747.0 / 2_147_483_647.0).abs() < 1e-15);
        assert_eq!(Rand::new(0).next(), 7.0 * 16807.0 / 2_147_483_647.0);
    }

    #[test]
    fn lines_and_shapes() {
        assert_eq!(to_d(&[(0.0, 0.0), (1.26, -0.04)]), "M0.0 0.0 L1.3 0.0");
        assert_eq!(length_of(&[(0.0, 0.0), (3.0, 4.0), (3.0, 10.0)]), 11.0);
        let square = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(encloses(&square, (5.0, 5.0)));
        assert!(!encloses(&square, (15.0, 5.0)));
        let line = hand(&[(0.0, 0.0), (28.0, 0.0)], 0.0, 1);
        assert_eq!(line.len(), 3);
        assert_eq!(line[2], (28.0, 0.0));
        assert_eq!(star(0.0, 0.0, 1.0).len(), 6);
    }
}
