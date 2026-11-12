//! The website's interactions: one meeting told as a set of drawing sheets. crates/site-gen renders the
//! HTML at build time, landing/style.css sets its look, and this animates it. Trunk builds it.

mod cards;
mod dots;
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

#[wasm_bindgen(start)]
pub fn start() {
    for e in util::qa("[data-version]") {
        util::set_text(&e, env!("CARGO_PKG_VERSION"));
    }
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
