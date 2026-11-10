//! Pictograms built from solid blocks and lines. Each part flies in from its own offset and settles when it
//! comes into view; hovering greys them, like pressing a button.

use crate::util::{
    add_class, data, frame, qa, qa_in, reduced, remove_class, set_style, svg_el, svg_parts,
};
use web_sys::Element;

pub fn icon(name: &str) -> String {
    match name {
        "export" => r#"<rect class="f" x="30" y="30" width="26" height="26" data-from="12,12"/><path class="s" d="M8 8 L36 36" stroke-width="7" data-from="-12,-12"/><path class="f" d="M8 8 h22 v8 h-14 v14 h-8z" data-from="-16,-16"/>"#.into(),
        "replay" => {
            let mut s = String::from(r#"<circle class="f" cx="32" cy="32" r="26" data-from="0,0" data-scale="0.4"/>"#);
            for i in 0..8 {
                let a = -std::f64::consts::FRAC_PI_2 + (f64::from(i) * std::f64::consts::PI) / 5.0;
                s += &format!(r#"<circle class="h" cx="{:.1}" cy="{:.1}" r="4" data-from="0,0" data-scale="0"/>"#, 32.0 + a.cos() * 17.0, 32.0 + a.sin() * 17.0);
            }
            s + r#"<circle class="h" cx="32" cy="32" r="5" data-from="0,0" data-scale="0"/>"#
        }
        "doc" => r#"<path class="f" d="M10 6 H40 L54 20 V58 H10 Z" data-from="0,14"/><path class="h" d="M40 6 V20 H54" data-from="10,-10"/><rect class="h" x="18" y="30" width="26" height="4" data-from="-20,0"/><rect class="h" x="18" y="40" width="18" height="4" data-from="-20,0"/>"#.into(),
        "lasso" => r#"<ellipse class="s" cx="34" cy="26" rx="22" ry="16" stroke-width="6" data-from="0,-12"/><rect class="r" x="10" y="40" width="12" height="12" data-from="-12,12"/><path class="s" d="M16 52 V60" stroke-width="5" data-from="0,10"/>"#.into(),
        "mic" => r#"<rect class="f" x="22" y="4" width="20" height="34" rx="10" data-from="0,-16"/><path class="s" d="M12 28 a20 20 0 0 0 40 0" stroke-width="5" data-from="0,10"/><rect class="f" x="29" y="48" width="6" height="10" data-from="0,14"/><rect class="f" x="18" y="56" width="28" height="5" data-from="0,18"/>"#.into(),
        "speakers" => r#"<circle class="f" cx="22" cy="24" r="14" data-from="-12,0"/><path class="f" d="M2 60 a20 18 0 0 1 40 0z" data-from="-12,10"/><circle class="s" cx="46" cy="20" r="9" stroke-width="5" data-from="12,0"/><path class="s" d="M34 46 a14 13 0 0 1 28 0" stroke-width="5" data-from="12,10"/>"#.into(),
        _ => r#"<rect class="s" x="9" y="9" width="46" height="46" stroke-width="7" data-from="0,-14"/>"#.into(),
    }
}

fn fill(svg: &Element, name: &str) {
    svg.set_text_content(None);
    for node in svg_parts(&icon(name)) {
        let _ = svg.append_child(&node);
    }
}

pub fn init() {
    let parts = |svg: &Element| qa_in(svg, "[data-from]");
    let seen = crate::util::observer(None, Some(0.5), move |entry, io| {
        if !entry.is_intersecting() {
            return;
        }
        let target = entry.target();
        remove_class(&target, "parked");
        let parts = parts(&target);
        wasm_bindgen_futures::spawn_local(async move {
            for (i, p) in parts.iter().enumerate() {
                set_style(p, "transition-delay", &format!("{}ms", i * 60));
            }
            frame().await;
            for p in &parts {
                set_style(p, "transform", "none");
            }
        });
        io.unobserve(&target);
    });
    for svg in qa("svg.pict[data-icon]") {
        fill(&svg, &data(&svg, "icon").unwrap_or_default());
        if reduced() {
            continue;
        }
        for p in parts(&svg) {
            let from = data(&p, "from").unwrap_or_else(|| "0,0".into());
            let (x, y) = from.split_once(',').unwrap_or(("0", "0"));
            set_style(&p, "transform-origin", "32px 32px");
            set_style(
                &p,
                "transform",
                &format!(
                    "translate({x}px, {y}px) scale({})",
                    data(&p, "scale").unwrap_or_else(|| "1".into())
                ),
            );
        }
        add_class(&svg, "parked");
        seen.observe(&svg);
    }
    // Small ones inside running text.
    for i in qa("[data-mini]") {
        let svg = svg_el(
            "svg",
            &[("class", "pict".into()), ("viewBox", "0 0 64 64".into())],
            None,
        );
        fill(&svg, &data(&i, "mini").unwrap_or_default());
        i.set_text_content(None);
        let _ = i.append_child(&svg);
    }
}
