//! A replay as one HTML file to send to someone who missed the meeting: nothing to install and nothing
//! fetched. Every step's ink is worked out here, so the file holds its own strokes and a small player.

use crate::Doc;
use boardcast_board::board_layout::StableLayout;
use boardcast_board::compose::{compose, PAPER};
use boardcast_board::glyphs::{Glyphs, Pt};
use boardcast_board::graph::Graph;
use boardcast_board::pen::{ink, outline, Rng};
use boardcast_board::steps::{apply_step, ink_only};
use boardcast_board::sugiyama::first_diagram;
use boardcast_meetings::PageImage;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Write;

const TEMPLATE: &str = include_str!("replay_page.html");
const PADDING: f64 = 48.0;

/// One board, as a meeting keeps it.
pub struct Page {
    pub graph: Graph,
    pub memory: Value,
    pub steps: Vec<Value>,
    pub image: Option<PageImage>,
}

fn path(points: &[Pt], close: bool) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let _ = write!(d, "{}{:.1} {:.1}", if i == 0 { 'M' } else { 'L' }, p[0], p[1]);
    }
    if close {
        d.push('Z');
    }
    d
}

/// The board at each step. Ink is kept once however many steps show it, and things start where they
/// finally ended up, as the older player does, so the board does not shuffle as it plays.
fn board_data(page: &Page, glyphs: &mut Glyphs) -> Value {
    let final_graph = ink_only(&page.graph);
    let mut graphs = vec![];
    if !page.steps.is_empty() {
        let mut g = Graph::default();
        graphs.push(g.clone());
        for step in &page.steps[..page.steps.len() - 1] {
            g = apply_step(&g, step);
            graphs.push(g.clone());
        }
    }
    graphs.push(final_graph);

    let mut layout = StableLayout::default();
    layout.restore(&page.memory);
    let mut known: HashMap<String, usize> = HashMap::new();
    let mut pieces: Vec<Value> = vec![];
    let mut frames = vec![];
    let mut area: Option<[f64; 4]> = None;
    for (i, g) in graphs.iter().enumerate() {
        let laid = layout.layout(g, &mut first_diagram);
        let (composed, _) = compose(g, &laid, glyphs);
        if !g.nodes.is_empty() || g.underlay().is_some() {
            let b = &laid.bounds;
            area = Some(area.map_or([b.x0, b.y0, b.x1, b.y1], |a| [a[0].min(b.x0), a[1].min(b.y0), a[2].max(b.x1), a[3].max(b.y1)]));
        }
        let mut shown = vec![];
        for piece in composed {
            let Some(style) = piece.lines.first().map(|l| l.style) else { continue };
            let id = format!("{}#{:06x}#{}", piece.key, style.rgb, style.alpha);
            let index = *known.entry(id).or_insert_with(|| {
                let mut r = Rng::new(&piece.seed);
                let strokes: Vec<Value> = piece
                    .lines
                    .iter()
                    .map(|line| {
                        let inked = ink(&line.points, line.width, line.wobble, &mut r);
                        json!([path(&outline(&inked, 1.0), true), path(&inked.center, false), (inked.length * 10.0).round() / 10.0, inked.width])
                    })
                    .collect();
                pieces.push(json!([format!("#{:06x}", style.rgb), style.alpha, style.under, strokes]));
                pieces.len() - 1
            });
            shown.push(index);
        }
        // Frame 0 is the empty board, and frame n is what step n had reached.
        let step = i.checked_sub(1).and_then(|s| page.steps.get(s));
        frames.push(json!({ "p": shown, "t": step.map_or(0.0, |s| s["t"].as_f64().unwrap_or(0.0)), "l": step.map_or(0, |s| s["line"].as_u64().unwrap_or(0)) }));
    }
    let a = area.unwrap_or([0.0, 0.0, 800.0, 500.0]);
    let bounds = [a[0] - PADDING, a[1] - PADDING, a[2] - a[0] + 2.0 * PADDING, a[3] - a[1] + 2.0 * PADDING];
    let image = page.image.as_ref().zip(page.graph.underlay()).map(|(image, (w, h))| json!({ "href": image.data_url(), "w": w, "h": h }));
    json!({ "box": bounds, "pieces": pieces, "frames": frames, "page": image })
}

/// JSON inside a script: `</script>` must not end it early, nor these two line separators break it.
fn script_safe(value: &Value) -> String {
    value.to_string().replace('<', "\\u003c").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029")
}

fn html_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn replay_page(doc: &Doc, pages: &[Page], glyphs: &mut Glyphs) -> String {
    let words = if doc.english {
        json!({ "play": "Play", "pause": "Pause", "end": "Skip to end", "replay": "boardcast replay", "summary": "Summary", "transcript": "Transcript", "count": "{} lines", "board": "Board", "boardN": "Board {}" })
    } else {
        json!({ "play": "播放", "pause": "暂停", "end": "跳到结尾", "replay": "boardcast 回放", "summary": "摘要", "transcript": "会议记录", "count": "{} 句", "board": "板书", "boardN": "第 {} 张板" })
    };
    let note = doc.omits_visuals.then(|| if doc.english { "Imported pages and free-edit content are not included." } else { "导入页和自由编辑内容没有包含在内。" });
    let lines: Vec<Value> = doc.lines.iter().map(|l| json!({ "s": l.speaker, "t": l.text, "m": l.manual })).collect();
    let boards: Vec<Value> = pages.iter().map(|page| board_data(page, glyphs)).collect();
    let data = json!({ "title": doc.title, "began": doc.began, "summary": doc.summary.trim(), "words": words, "note": note, "lines": lines, "boards": boards });
    let licenses = format!(
        "Japanese stroke data: KanjiVG, Copyright (C) 2009/2010/2011 Ulrich Apel. https://kanjivg.tagaini.net/ adapted centerlines, CC BY-SA 3.0 https://creativecommons.org/licenses/by-sa/3.0/\n{}\n{}",
        include_str!("../../../public/licenses/handwritten-OFL.txt"),
        include_str!("../../../public/licenses/handwritten-cascadia.txt"),
    );
    let suffix = if doc.english { "Replay" } else { "回放" };
    // The meeting's own words go in last and apart, so they can never be taken for a marker.
    let (head, tail) = TEMPLATE.split_once("__DATA__").expect("the template has a place for the data");
    let head = head
        .replacen("__LANG__", if doc.english { "en" } else { "zh-CN" }, 1)
        .replacen("__PAPER__", &format!("#{PAPER:06x}"), 1)
        .replacen("__LICENSES__", &licenses.replace("-->", "- ->"), 1)
        .replacen("__TITLE__", &html_text(&format!("{} · {suffix}", doc.title)), 1);
    format!("{head}{}{tail}", script_safe(&data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Said;
    use boardcast_board::graph::parse_ops;
    use boardcast_board::steps::diff;
    use std::path::Path;

    fn glyphs() -> Glyphs {
        Glyphs::from_checkout(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).unwrap()
    }

    fn page(rounds: &[&str]) -> Page {
        let (mut g, mut steps) = (Graph::default(), vec![]);
        for (i, ops) in rounds.iter().enumerate() {
            let before = g.clone();
            g.apply(&parse_ops(ops));
            steps.push(json!({ "t": 1000 + i * 1000, "line": i + 1, "d": diff(&before, &g).unwrap() }));
        }
        Page { graph: g, memory: Value::Null, steps, image: None }
    }

    fn doc(title: &str, text: &str) -> Doc {
        Doc {
            title: title.into(),
            exported: String::new(),
            began: "2026-01-02 03:04".into(),
            summary: "### 结论\n- 好".into(),
            boards: vec![],
            lines: vec![Said { speaker: Some("A".into()), text: text.into(), page: 0, manual: false }],
            english: false,
            omits_visuals: false,
        }
    }

    fn embedded(html: &str) -> Value {
        let start = html.find("const DATA=").unwrap() + "const DATA=".len();
        let end = start + html[start..].find(";</script>").unwrap();
        serde_json::from_str(&html[start..end]).unwrap()
    }

    #[test]
    fn every_step_has_a_frame_and_the_last_shows_the_whole_board() {
        let html = replay_page(&doc("评审", "说"), &[page(&["T 「甲」", "L 「甲」 「乙」", "E 「丙」 -> 「丁」"])], &mut glyphs());
        let data = embedded(&html);
        let board = &data["boards"][0];
        let frames = board["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 4, "the empty board and three steps");
        assert!(frames[0]["p"].as_array().unwrap().is_empty());
        let sizes: Vec<usize> = frames.iter().map(|f| f["p"].as_array().unwrap().len()).collect();
        assert!(sizes.windows(2).all(|w| w[0] < w[1]), "{sizes:?}");
        assert_eq!(frames[3]["l"], 3);
        assert_eq!(frames[3]["t"].as_f64(), Some(3000.0));
    }

    #[test]
    fn ink_shown_in_many_steps_is_stored_once() {
        let html = replay_page(&doc("t", "x"), &[page(&["T 「甲」", "T 「乙」"])], &mut glyphs());
        let data = embedded(&html);
        let board = &data["boards"][0];
        let frames = board["frames"].as_array().unwrap();
        let last = frames[2]["p"].as_array().unwrap();
        assert_eq!(board["pieces"].as_array().unwrap().len(), last.len(), "nothing moved, so nothing is stored twice");
        for shown in frames[1]["p"].as_array().unwrap() {
            assert!(last.contains(shown));
        }
    }

    #[test]
    fn a_meeting_with_no_record_still_shows_its_board() {
        let mut g = Graph::default();
        g.apply(&parse_ops("T 「甲」"));
        let html = replay_page(&doc("t", "x"), &[Page { graph: g, memory: Value::Null, steps: vec![], image: None }], &mut glyphs());
        let frames = embedded(&html)["boards"][0]["frames"].as_array().unwrap().clone();
        assert_eq!(frames.len(), 1);
        assert!(!frames[0]["p"].as_array().unwrap().is_empty());
    }

    #[test]
    fn meeting_text_cannot_escape_the_page() {
        let hostile = "</script><script>alert(1)</script>\u{2028}<!-- __DATA__ __TITLE__";
        let html = replay_page(&doc("</title><img src=x>", hostile), &[page(&["T 「甲」"])], &mut glyphs());
        assert!(html.contains("<title>&lt;/title&gt;&lt;img src=x&gt; · 回放</title>"));
        assert_eq!(html.matches("</script>").count(), 2, "only the page's own scripts close");
        assert!(!html.contains('\u{2028}'));
        let data = embedded(&html);
        assert_eq!(data["lines"][0]["t"], hostile);
        assert_eq!(data["title"], "</title><img src=x>");
    }

    #[test]
    fn a_title_that_looks_like_a_marker_is_only_a_title() {
        let title = "__DATA__ __LICENSES__ __PAPER__";
        let html = replay_page(&doc(title, "x"), &[page(&["T 「甲」"])], &mut glyphs());
        assert!(html.contains(&format!("<title>{title} · 回放</title>")));
        assert_eq!(embedded(&html)["title"], title);
        assert!(html.contains("KanjiVG"));
    }

    #[test]
    fn nothing_is_fetched_and_the_notices_stay_in_their_comment() {
        let html = replay_page(&doc("t", "x"), &[page(&["T 「甲」"])], &mut glyphs());
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("KanjiVG"));
        let start = html.find("<!--").unwrap();
        let end = start + html[start..].find("-->").unwrap();
        assert!(html[end..].contains("<style>"), "the comment ends before the page goes on");
        assert!(!html.contains("__LICENSES__") && !html.contains("__PAPER__") && !html.contains("__LANG__"));
    }
}
