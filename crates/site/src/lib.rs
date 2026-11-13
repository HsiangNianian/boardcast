//! The website's interactions: one meeting told as a set of drawing sheets. crates/site-gen renders the
//! HTML at build time, landing/style.css sets its look, and this animates it. Trunk builds it.

mod cards;
mod dots;
mod ground;
mod hero;
mod lasso;
mod meeting;
mod now;
mod pen;
mod pict;
mod sheet;
mod signals;
mod story;
mod util;

use wasm_bindgen::prelude::*;

/// The demo plays in its window below the opening on a wide screen, and behind the opening on a phone. Each
/// frame gets its page only once its layout shows, so a screen loads one demo, not two; a window resized
/// across the line loads the other then.
fn demo_frames() {
    let load = || {
        let phone = util::win().match_media("(max-width: 760px)").ok().flatten().is_some_and(|m| m.matches());
        let Some(frame) = util::doc().get_element_by_id(if phone { "hero-board" } else { "live-demo" }) else { return };
        if !frame.has_attribute("src") {
            if let Some(src) = frame.get_attribute("data-src") {
                let _ = frame.set_attribute("src", &src);
            }
        }
    };
    load();
    util::on_passive(&util::win(), "resize", move |_| load());
}

#[wasm_bindgen(start)]
pub fn start() {
    for e in util::qa("[data-version]") {
        util::set_text(&e, env!("CARGO_PKG_VERSION"));
    }
    demo_frames();
    ground::init();
    pict::init();
    sheet::init();
    lasso::init();
    signals::init();
    cards::init();
    dots::init();
    // Board words are measured, so they wait for the handwriting font.
    wasm_bindgen_futures::spawn_local(async {
        let _ = wasm_bindgen_futures::JsFuture::from(util::doc().fonts().load("22px Excalifont")).await;
        hero::init();
        story::init();
        now::init();
    });
}
