//! The fonts the desktop app carries, built into the program so that the interface and exported PDFs look the
//! same on every computer, whatever fonts it has (#309). All are under the SIL Open Font License; the
//! licenses are in `files/` and are copied into every package.
//!
//! - DM Sans: interface text.
//! - JetBrains Mono: numbers, times and shortcuts.
//! - Noto Sans CJK SC, regular and bold: Chinese, Japanese and Korean, and Cyrillic, which DM Sans lacks, in
//!   the interface and in PDFs. One region's font covers every CJK character, kana, Hangul and Cyrillic, in
//!   Simplified Chinese shapes. The collection with every region's shapes cannot be used: macOS reads only the
//!   first face of a font given from memory.
//!
//! The fonts the handwriting strokes were made from (Excalifont, Cascadia Code, Xiaolai, KanjiVG, Hanzi
//! Writer) are not here: only the strokes derived from them ship, in the glyph pack.

/// A font built into the program.
pub struct Embedded {
    /// What it is called in reports.
    pub name: &'static str,
    /// The family name text is set in.
    pub family: &'static str,
    pub bold: bool,
    pub bytes: &'static [u8],
    /// Characters it must draw, as checked by [`check`].
    pub covers: &'static str,
}

/// The family that Chinese, Japanese, Korean and Cyrillic text falls back to.
pub const CJK_FAMILY: &str = "Noto Sans CJK SC";

pub const DM_SANS: Embedded = Embedded { name: "DM Sans", family: "DM Sans", bold: false, bytes: include_bytes!("../files/DMSans.ttf"), covers: "Boardcast éü" };
pub const JETBRAINS_MONO: Embedded = Embedded { name: "JetBrains Mono", family: "JetBrains Mono", bold: false, bytes: include_bytes!("../files/JetBrainsMono.ttf"), covers: "0123:45" };
pub const CJK_REGULAR: Embedded = Embedded { name: "Noto Sans CJK SC Regular", family: CJK_FAMILY, bold: false, bytes: include_bytes!("../files/NotoSansCJKsc-Regular.otf"), covers: SAMPLE };
pub const CJK_BOLD: Embedded = Embedded { name: "Noto Sans CJK SC Bold", family: CJK_FAMILY, bold: true, bytes: include_bytes!("../files/NotoSansCJKsc-Bold.otf"), covers: SAMPLE };

/// Simplified and Traditional Chinese, Japanese, Korean and Russian: every language the interface is in
/// that DM Sans cannot draw.
const SAMPLE: &str = "会议記錄ひらがなカタカナ회의록Встреча";

pub const ALL: [&Embedded; 4] = [&DM_SANS, &JETBRAINS_MONO, &CJK_REGULAR, &CJK_BOLD];

/// What is wrong with a built-in font, if anything: it must parse, have the family name the interface asks
/// for, and draw every character it is there for.
pub fn problem(font: &Embedded) -> Option<String> {
    let face = match ttf_parser::Face::parse(font.bytes, 0) {
        Ok(face) => face,
        Err(e) => return Some(format!("does not parse: {e}")),
    };
    let names: Vec<String> = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::FAMILY || n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
        .filter_map(|n| n.to_string())
        .collect();
    if !names.iter().any(|n| n == font.family) {
        return Some(format!("is named {names:?}, not {}", font.family));
    }
    let missing: String = font.covers.chars().filter(|c| !c.is_whitespace() && face.glyph_index(*c).is_none_or(|g| g.0 == 0)).collect();
    (!missing.is_empty()).then(|| format!("cannot draw {missing}"))
}

/// Every built-in font with what is wrong with it, for the self-check.
pub fn check() -> Vec<(&'static str, Option<String>)> {
    ALL.iter().map(|f| (f.name, problem(f))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_font_loads_and_draws_what_it_is_for() {
        for (name, problem) in check() {
            assert!(problem.is_none(), "{name} {}", problem.unwrap_or_default());
        }
    }

    #[test]
    fn a_font_missing_a_character_is_reported() {
        let latin_only = Embedded { covers: "中", ..DM_SANS };
        assert_eq!(problem(&latin_only).as_deref(), Some("cannot draw 中"));
        let misnamed = Embedded { family: "Helvetica", ..DM_SANS };
        assert!(problem(&misnamed).is_some_and(|p| p.starts_with("is named")));
    }
}
