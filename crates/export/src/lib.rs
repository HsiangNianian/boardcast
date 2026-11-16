//! Writing a meeting out as a file. Everything is taken from a frozen copy, so carrying on with the
//! meeting while a file is written cannot change it.

pub mod animate;
pub mod ink;
pub mod markdown;
pub mod pdf;
pub mod replay_page;
mod webm;

use anyhow::{bail, Context, Result};
use boardcast_board::glyphs::{Glyphs, HanziSource};
use boardcast_board::graph::Graph;
use boardcast_meetings::{Live, PageImage, ViewOnly};
use chrono::TimeZone;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A meeting's kept sound, ready to go into a file: its pieces as `(when each began, in ms since the epoch,
/// and how long it is, in ms)`, laid end to end, and the whole of it as Opus packets.
pub struct Sound {
    pub pieces: Vec<(f64, f64)>,
    /// The identification header, the look-ahead in samples at 16 kHz, and evenly long packets.
    pub head: Vec<u8>,
    pub pre_skip: u64,
    pub packet_ms: u64,
    pub packets: Vec<Vec<u8>>,
}

impl Sound {
    fn pre_skip_ns(&self) -> u64 {
        self.pre_skip * 1_000_000_000 / 16_000
    }

    /// The sound alone, as a WebM file a browser plays.
    fn webm(&self) -> Vec<u8> {
        let audio = webm::Audio { head: &self.head, pre_skip_ns: self.pre_skip_ns(), packet_ms: self.packet_ms, packets: &self.packets };
        webm::write(None, Some(audio), self.packets.len() as f64 * self.packet_ms as f64)
    }

    /// Where each step falls in the sound, in ms.
    fn times_ms(&self, steps: &[Value]) -> Vec<f64> {
        boardcast_board::steps::sound_times(steps, &self.pieces).into_iter().map(|s| s * 1000.0).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Markdown,
    Png,
    Svg,
    AnimatedSvg,
    /// The animated board as a GIF, for chats and documents that don't play an SVG.
    Gif,
    /// The animated board as a WebM video (AV1): sharper and smaller than the GIF, for browsers and sites.
    Webm,
    /// The whole meeting as a video with its kept sound, in real time.
    LiveVideo,
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
            Self::Webm | Self::LiveVideo => "webm",
            Self::Pdf => "pdf",
            Self::ReplayPage => "html",
        }
    }

    /// Markdown and PDF cover the whole meeting; a picture is of the board on screen.
    pub fn needs_a_drawn_board(self) -> bool {
        matches!(self, Self::Png | Self::Svg | Self::AnimatedSvg | Self::Gif | Self::Webm)
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
            "webm" => Some(Self::Webm),
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
                speaker: l["speaker"].as_str().filter(|s| !s.is_empty()).map(str::to_string),
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
    /// Every board with its record. A replay page lays out each step, so it does that on its own glyphs.
    replay_pages: Vec<replay_page::Page>,
    hanzi: HanziSource,
    /// The meeting's kept sound, for the formats that carry it, when the caller has read it.
    pub sound: Option<Sound>,
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
/// `images` are each board's page image, in the order of `Live::board_images`; `asset` reads any other
/// picture the meeting keeps, by its name, as clips brought from elsewhere are.
pub fn freeze(live: &Live, english: bool, glyphs: &mut Glyphs, hanzi: HanziSource, images: &[Option<PageImage>], asset: &dyn Fn(&str) -> Option<PageImage>) -> Frozen {
    let boards = live.board_pages();
    let image_of = |i: usize| images.get(i).and_then(Option::as_ref);
    // Each board's clip pictures come from its own page, or from a picture the meeting keeps.
    let sources: Vec<ink::Sources> = boards
        .iter()
        .enumerate()
        .map(|(i, (graph, _))| {
            let mut found = ink::Sources::new();
            if let Some(page) = image_of(i) {
                found.insert(String::new(), page.clone());
            }
            for clip in graph.nodes.iter().filter_map(|n| n.clip()).filter(|c| !c.src.is_empty()) {
                if !found.contains_key(&clip.src) {
                    if let Some(image) = asset(&clip.src) {
                        found.insert(clip.src.clone(), image);
                    }
                }
            }
            found
        })
        .collect();
    let began = live.created_at().and_then(|ms| chrono::Local.timestamp_millis_opt(ms as i64).single()).unwrap_or_else(chrono::Local::now);
    let pages = boards.iter().enumerate().map(|(i, (graph, memory))| ink::compose_board(graph, memory, image_of(i), &sources[i], glyphs)).collect();
    let replay_pages = boards
        .iter()
        .zip(live.board_steps())
        .enumerate()
        .map(|(i, ((graph, memory), steps))| replay_page::Page { graph: graph.clone(), memory: memory.clone(), steps, image: image_of(i).cloned(), sources: sources[i].clone() })
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
        replay_pages,
        hanzi,
        sound: None,
    }
}

impl Frozen {
    pub fn render(self, kind: Kind) -> Result<Output> {
        let Self { doc, pages, current, replay_pages, hanzi, sound } = self;
        let text_only = |bytes: Vec<u8>| Output { bytes, text_left_out: false };
        match kind {
            Kind::Markdown => Ok(text_only(markdown::markdown(&doc, current).into_bytes())),
            Kind::Pdf => {
                let drawn: Vec<ink::Drawn> = pages.into_iter().map(ink::Composed::draw).collect();
                pdf::pdf(&doc, &drawn)
            }
            Kind::ReplayPage => {
                let mut glyphs = Glyphs::load(hanzi)?;
                Ok(text_only(replay_page::replay_page(&doc, &replay_pages, &mut glyphs, sound.as_ref()).into_bytes()))
            }
            Kind::LiveVideo => {
                let sound = sound.context("This meeting kept no sound.")?;
                let mut glyphs = Glyphs::load(hanzi)?;
                // Each board in real time: its steps where they were heard, and the board as kept after the last.
                let boards: Vec<animate::Replay> = replay_pages
                    .iter()
                    .map(|page| {
                        let mut starts = sound.times_ms(&page.steps);
                        starts.push(starts.last().copied().unwrap_or(0.0));
                        animate::timed(page, &mut glyphs, Some(&starts))
                    })
                    .collect();
                if boards.iter().all(animate::Replay::is_empty) {
                    bail!("The board is empty.");
                }
                // The board in view follows the meeting from one to another.
                let timeline = boardcast_board::steps::timeline(&replay_pages.iter().map(|p| p.steps.clone()).collect::<Vec<_>>());
                let steps: Vec<Value> = timeline.iter().map(|(_, s)| s.clone()).collect();
                let mut shown: Vec<(f64, usize)> = sound.times_ms(&steps).into_iter().zip(timeline.iter().map(|(b, _)| *b)).collect();
                if shown.is_empty() {
                    shown.push((0.0, current));
                }
                Ok(text_only(animate::live_webm(&boards, &shown, &sound)?))
            }
            // Both replay the board on screen step by step, which lays it out many times over.
            Kind::AnimatedSvg | Kind::Gif | Kind::Webm => {
                let page = replay_pages.get(current).context("no board to export")?;
                let mut glyphs = Glyphs::load(hanzi)?;
                let replay = animate::replay(page, &mut glyphs);
                if replay.is_empty() {
                    bail!("The board is empty.");
                }
                Ok(text_only(match kind {
                    Kind::Gif => animate::gif(&replay)?,
                    Kind::Webm => animate::webm(&replay)?,
                    _ => animate::animated_svg(&replay, &doc.title).into_bytes(),
                }))
            }
            Kind::Png | Kind::Svg => {
                let page = pages.into_iter().nth(current).context("no board to export")?;
                let drawn = page.draw();
                if drawn.strokes.is_empty() && drawn.page.is_none() {
                    bail!("The board is empty.");
                }
                Ok(text_only(if kind == Kind::Png { ink::png(&drawn)? } else { ink::svg(&drawn, &doc.title).into_bytes() }))
            }
        }
    }
}

/// `boardcast-20260102-0304.md`, as the older app names them.
pub fn file_name(kind: Kind) -> String {
    format!("boardcast-{}.{}", chrono::Local::now().format("%Y%m%d-%H%M"), kind.extension())
}

/// A picker may hand back the name without the extension that was suggested.
pub fn with_extension(path: PathBuf, kind: Kind) -> PathBuf {
    let has = path.to_str().is_some_and(|p| p.to_ascii_lowercase().ends_with(&format!(".{}", kind.extension())));
    if has {
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
    let name = path.file_name().context("the path has no file name")?.to_string_lossy();
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
        assert_eq!(with_extension("/a/notes".into(), Kind::Pdf), PathBuf::from("/a/notes.pdf"));
        assert_eq!(with_extension("/a/notes.PDF".into(), Kind::Pdf), PathBuf::from("/a/notes.PDF"));
        assert_eq!(with_extension("/a/v1.2".into(), Kind::Svg), PathBuf::from("/a/v1.2.svg"));
        assert_eq!(with_extension("/a/notes".into(), Kind::AnimatedSvg), PathBuf::from("/a/notes.anim.svg"));
        assert_eq!(with_extension("/a/notes.anim.svg".into(), Kind::AnimatedSvg), PathBuf::from("/a/notes.anim.svg"));
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
        let lines = Said::from_stored(&[serde_json::json!({ "text": "a", "speaker": "A", "page": 1 }), serde_json::json!({ "text": "b", "manual": true })]);
        assert_eq!(lines[0], Said { speaker: Some("A".into()), text: "a".into(), page: 1, manual: false });
        assert!(lines[1].manual);
    }
}
