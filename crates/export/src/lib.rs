//! Writing a meeting out as a file. Everything is taken from a frozen copy, so carrying on with the
//! meeting while a file is written cannot change it.

pub mod animate;
pub mod ink;
pub mod markdown;
pub mod pdf;
pub mod replay_page;

use anyhow::{bail, Context, Result};
use boardcast_board::glyphs::{Glyphs, HanziSource};
use boardcast_board::graph::Graph;
use boardcast_board::steps::ink_only;
use boardcast_meetings::{Live, PageImage, ViewOnly};
use chrono::TimeZone;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Markdown,
    Png,
    Svg,
    AnimatedSvg,
    /// The animated board as a GIF, for chats and documents that don't play an SVG.
    Gif,
    Pdf,
    ReplayPage,
}

impl Kind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Png => "png",
            Self::Svg => "svg",
            Self::AnimatedSvg => "anim.svg",
            Self::Gif => "gif",
            Self::Pdf => "pdf",
            Self::ReplayPage => "html",
        }
    }

    /// Markdown and PDF cover the whole meeting; a picture is of the board on screen.
    pub fn needs_a_drawn_board(self) -> bool {
        matches!(self, Self::Png | Self::Svg | Self::AnimatedSvg | Self::Gif)
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        if path.to_str()?.to_ascii_lowercase().ends_with(".anim.svg") {
            return Some(Self::AnimatedSvg);
        }
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "md" => Some(Self::Markdown),
            "png" => Some(Self::Png),
            "svg" => Some(Self::Svg),
            "pdf" => Some(Self::Pdf),
            "gif" => Some(Self::Gif),
            "html" => Some(Self::ReplayPage),
            _ => None,
        }
    }
}

/// One line of the transcript.
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    pub speaker: Option<String>,
    pub text: String,
    pub page: u64,
    /// A note about a hand-made change rather than something said.
    pub manual: bool,
}

impl Said {
    pub fn from_stored(lines: &[Value]) -> Vec<Self> {
        lines
            .iter()
            .map(|l| Self {
                speaker: l["speaker"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                text: l["text"].as_str().unwrap_or_default().to_string(),
                page: l["page"].as_u64().unwrap_or(0),
                manual: l["manual"].as_bool().unwrap_or(false),
            })
            .collect()
    }
}

/// The words of a meeting, for the formats that carry text.
#[derive(Clone, Debug)]
pub struct Doc {
    pub title: String,
    /// When the file is made, for Markdown.
    pub exported: String,
    /// When the meeting began, for PDF.
    pub began: String,
    pub summary: String,
    pub boards: Vec<Graph>,
    pub lines: Vec<Said>,
    /// The language of the headings, which follows the interface as in the older app.
    pub english: bool,
    /// The meeting has imported pages or free-edit content that cannot be exported yet.
    pub omits_visuals: bool,
}

/// Everything needed to write any format, captured at one moment.
pub struct Frozen {
    pub doc: Doc,
    pages: Vec<ink::Composed>,
    current: usize,
    /// The record of the board on screen, and that board laid out without drafts, for the animation.
    steps: Vec<Value>,
    settled: ink::Composed,
    settled_nodes: Vec<Value>,
    /// Every board with its record. A replay page lays out each step, so it does that on its own glyphs.
    replay_pages: Vec<replay_page::Page>,
    hanzi: HanziSource,
}

/// A finished file, and whether anything had to be left out of it.
pub struct Output {
    pub bytes: Vec<u8>,
    pub text_left_out: bool,
}

fn stamp(when: chrono::DateTime<chrono::Local>) -> String {
    when.format("%Y-%m-%d %H:%M").to_string()
}

/// Laying out needs the glyphs, which the caller keeps; the slow drawing waits for `render`.
/// `images` are each board's page image, in the order of `Live::board_images`.
pub fn freeze(
    live: &Live,
    english: bool,
    glyphs: &mut Glyphs,
    hanzi: HanziSource,
    images: &[Option<PageImage>],
) -> Frozen {
    let boards = live.board_pages();
    let image_of = |i: usize| images.get(i).and_then(Option::as_ref);
    let began = live
        .created_at()
        .and_then(|ms| chrono::Local.timestamp_millis_opt(ms as i64).single())
        .unwrap_or_else(chrono::Local::now);
    let pages = boards
        .iter()
        .enumerate()
        .map(|(i, (graph, memory))| ink::compose_board(graph, memory, image_of(i), glyphs))
        .collect();
    let settled_graph = ink_only(&live.graph);
    let settled = ink::compose_board(&settled_graph, &live.memory, image_of(live.page()), glyphs);
    let replay_pages = boards
        .iter()
        .zip(live.board_steps())
        .enumerate()
        .map(|(i, ((graph, memory), steps))| replay_page::Page {
            graph: graph.clone(),
            memory: memory.clone(),
            steps,
            image: image_of(i).cloned(),
        })
        .collect();
    let settled_nodes = settled_graph
        .nodes
        .iter()
        .filter_map(|n| serde_json::to_value(n).ok())
        .collect();
    Frozen {
        doc: Doc {
            title: live.title().to_string(),
            exported: stamp(chrono::Local::now()),
            began: stamp(began),
            summary: live.summary().to_string(),
            boards: boards.into_iter().map(|(graph, _)| graph).collect(),
            lines: Said::from_stored(&live.lines),
            english,
            omits_visuals: live.read_only == Some(ViewOnly::ImportedContent),
        },
        pages,
        current: live.page(),
        steps: live.steps.clone(),
        settled,
        settled_nodes,
        replay_pages,
        hanzi,
    }
}

impl Frozen {
    pub fn render(self, kind: Kind) -> Result<Output> {
        let Self {
            doc,
            pages,
            current,
            steps,
            settled,
            settled_nodes,
            replay_pages,
            hanzi,
        } = self;
        let text_only = |bytes: Vec<u8>| Output {
            bytes,
            text_left_out: false,
        };
        match kind {
            Kind::Markdown => Ok(text_only(markdown::markdown(&doc, current).into_bytes())),
            Kind::Pdf => {
                let drawn: Vec<ink::Drawn> = pages.into_iter().map(ink::Composed::draw).collect();
                pdf::pdf(&doc, &drawn)
            }
            Kind::ReplayPage => {
                let mut glyphs = Glyphs::load(hanzi)?;
                Ok(text_only(
                    replay_page::replay_page(&doc, &replay_pages, &mut glyphs).into_bytes(),
                ))
            }
            Kind::AnimatedSvg => {
                if settled.pieces.is_empty() && settled.page.is_none() {
                    bail!("The board is empty.");
                }
                Ok(text_only(
                    animate::animated_svg(&settled, &settled_nodes, &steps, &doc.title)
                        .into_bytes(),
                ))
            }
            Kind::Gif => {
                if settled.pieces.is_empty() && settled.page.is_none() {
                    bail!("The board is empty.");
                }
                Ok(text_only(animate::gif(&settled, &settled_nodes, &steps)?))
            }
            Kind::Png | Kind::Svg => {
                let page = pages
                    .into_iter()
                    .nth(current)
                    .context("no board to export")?;
                let drawn = page.draw();
                if drawn.strokes.is_empty() && drawn.page.is_none() {
                    bail!("The board is empty.");
                }
                Ok(text_only(if kind == Kind::Png {
                    ink::png(&drawn)?
                } else {
                    ink::svg(&drawn, &doc.title).into_bytes()
                }))
            }
        }
    }
}

/// `boardcast-20260102-0304.md`, as the older app names them.
pub fn file_name(kind: Kind) -> String {
    format!(
        "boardcast-{}.{}",
        chrono::Local::now().format("%Y%m%d-%H%M"),
        kind.extension()
    )
}

/// A picker may hand back the name without the extension that was suggested.
pub fn with_extension(path: PathBuf, kind: Kind) -> PathBuf {
    if Kind::from_path(&path) == Some(kind) {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".");
        name.push(kind.extension());
        name.into()
    }
}

/// Written beside the target and moved into place, so a failure never leaves half a file behind.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path
        .file_name()
        .context("the path has no file name")?
        .to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.part"));
    std::fs::write(&temporary, bytes).with_context(|| format!("writing {}", path.display()))?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_extension_is_added_and_a_right_one_kept() {
        assert_eq!(
            with_extension("/a/notes".into(), Kind::Pdf),
            PathBuf::from("/a/notes.pdf")
        );
        assert_eq!(
            with_extension("/a/notes.PDF".into(), Kind::Pdf),
            PathBuf::from("/a/notes.PDF")
        );
        assert_eq!(
            with_extension("/a/v1.2".into(), Kind::Svg),
            PathBuf::from("/a/v1.2.svg")
        );
        assert_eq!(
            with_extension("/a/notes".into(), Kind::AnimatedSvg),
            PathBuf::from("/a/notes.anim.svg")
        );
        assert_eq!(
            with_extension("/a/notes.anim.svg".into(), Kind::AnimatedSvg),
            PathBuf::from("/a/notes.anim.svg")
        );
        assert_eq!(Kind::from_path(Path::new("/a/notes.svg")), Some(Kind::Svg));
    }

    #[test]
    fn a_file_replaces_the_old_one_and_leaves_nothing_else() {
        let dir = std::env::temp_dir().join(format!("boardcast-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.md");
        write_file(&path, b"one").unwrap();
        write_file(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stored_lines_keep_their_board_and_notes_stay_marked() {
        let lines = Said::from_stored(&[
            serde_json::json!({ "text": "a", "speaker": "A", "page": 1 }),
            serde_json::json!({ "text": "b", "manual": true }),
        ]);
        assert_eq!(
            lines[0],
            Said {
                speaker: Some("A".into()),
                text: "a".into(),
                page: 1,
                manual: false
            }
        );
        assert!(lines[1].manual);
    }
}
