//! The cards of "during the meeting": each one shows, small, what the app really does.

use crate::util::{
    append, el, hand, interval, lasso_loop, q, qa, reduced, set_attr, set_style, set_text, to_d,
    toggle, wait,
};

pub fn init() {
    // Circle two things, then link them, over and over.
    let (l1, l2, lk) = (q("#l1"), q("#l2"), q("#lk"));
    set_attr(
        &l1,
        "d",
        to_d(&lasso_loop(160.0, 60.0, 120.0, 34.0, 5, 1.12)),
    );
    set_attr(
        &l2,
        "d",
        to_d(&lasso_loop(380.0, 118.0, 64.0, 30.0, 8, 1.12)),
    );
    set_attr(
        &lk,
        "d",
        to_d(&hand(&[(270.0, 82.0), (330.0, 106.0)], 3.0, 4)),
    );
    if !reduced() {
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                for p in [&l1, &l2, &lk] {
                    set_style(p, "transition", "none");
                    set_style(p, "stroke-dasharray", "1");
                    set_style(p, "stroke-dashoffset", "1");
                    set_style(p, "opacity", "1");
                }
                // Read layout so the reset applies before the transitions start.
                l1.get_bounding_client_rect();
                wait(300).await;
                set_style(&l1, "transition", "stroke-dashoffset .9s ease");
                set_style(&l1, "stroke-dashoffset", "0");
                wait(900).await;
                set_style(&l2, "transition", "stroke-dashoffset .8s ease");
                set_style(&l2, "stroke-dashoffset", "0");
                wait(1000).await;
                set_style(&lk, "transition", "stroke-dashoffset .45s ease");
                set_style(&lk, "stroke-dashoffset", "0");
                wait(600).await;
                for p in [&l1, &l2] {
                    set_style(p, "transition", "opacity .6s");
                    set_style(p, "opacity", "0");
                }
                wait(2600).await;
            }
        });
    }

    // Voices take turns.
    for w in qa(".voice .wave") {
        for i in 0..14 {
            let b = el("i", "", "");
            set_style(
                &b,
                "animation-delay",
                &format!("{}s", (f64::from(i) * 0.11) % 0.7),
            );
            append(&w, &b);
        }
    }
    let voices = qa(".voice");
    if !reduced() {
        let mut who = 0;
        interval(1700, move || {
            who = (who + 1) % voices.len();
            for (i, v) in voices.iter().enumerate() {
                toggle(v, "quiet", i != who);
            }
        });
    }

    // Replay: a scrubber walks along the meeting's speech.
    let bars = q("#bars");
    let label = q("#bt");
    let n = 44;
    let items: Vec<_> = (0..n)
        .map(|i| {
            let f = f64::from(i);
            let b = el("i", "", "");
            set_style(
                &b,
                "height",
                &format!(
                    "{}%",
                    16.0 + ((f * 1.7).sin() * 66.0 + (f * 0.6).cos() * 18.0).abs()
                ),
            );
            append(&bars, &b);
            b
        })
        .collect();
    let mut pos = 8;
    let mut step = move || {
        pos = (pos + 1) % n;
        for (i, b) in items.iter().enumerate() {
            toggle(b, "past", i as i32 <= pos);
        }
        let s = (f64::from(pos) / f64::from(n - 1) * 2910.0).round() as i64;
        set_text(&label, &format!("{:02}:{:02}", s / 60, s % 60));
    };
    step();
    if !reduced() {
        interval(240, step);
    }
}
