//! Character -> strokes, using the data files the current app ships.
//! Points are in font units: size = 1, (0.5, 0.5) is the middle of the character.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type Pt = [f32; 2];
pub type Stroke = Vec<Pt>;

#[derive(Clone, Debug, Deserialize)]
pub struct Glyph {
    pub strokes: Vec<Stroke>,
    pub advance: f32,
    /// English strokes keep full weight; Chinese and Japanese are thinner by default.
    #[serde(default)]
    pub latin: bool,
}

/// Chinese punctuation has no stroke data, so a few hand-picked strokes stand in, as in glyphs.ts.
const PUNCT: &[(char, &[&[Pt]], f32)] = &[
    ('·', &[&[[0.24, 0.47], [0.26, 0.5]]], 0.5),
    ('–', &[&[[0.08, 0.55], [0.65, 0.54]]], 0.75),
    ('，', &[&[[0.22, 0.78], [0.26, 0.84], [0.2, 0.95]]], 0.5),
    ('。', &[&[[0.22, 0.76], [0.3, 0.74], [0.34, 0.82], [0.27, 0.88], [0.2, 0.83], [0.22, 0.76]]], 0.5),
    ('、', &[&[[0.18, 0.74], [0.3, 0.86]]], 0.5),
    ('：', &[&[[0.25, 0.38], [0.26, 0.42]], &[[0.25, 0.74], [0.26, 0.78]]], 0.5),
    ('；', &[&[[0.25, 0.38], [0.26, 0.42]], &[[0.27, 0.74], [0.22, 0.92]]], 0.5),
    ('？', &[&[[0.28, 0.3], [0.4, 0.18], [0.58, 0.2], [0.62, 0.34], [0.45, 0.5], [0.44, 0.64]], &[[0.44, 0.8], [0.45, 0.84]]], 0.8),
    ('！', &[&[[0.4, 0.18], [0.38, 0.64]], &[[0.38, 0.8], [0.39, 0.84]]], 0.6),
    ('（', &[&[[0.5, 0.1], [0.32, 0.4], [0.32, 0.7], [0.5, 0.98]]], 0.6),
    ('）', &[&[[0.2, 0.1], [0.38, 0.4], [0.38, 0.7], [0.2, 0.98]]], 0.6),
    ('「', &[&[[0.5, 0.12], [0.3, 0.12], [0.3, 0.6]]], 0.6),
    ('」', &[&[[0.3, 0.5], [0.3, 0.95], [0.1, 0.95]]], 0.6),
    ('—', &[&[[0.05, 0.55], [0.95, 0.53]]], 1.0),
    ('→', &[&[[0.05, 0.55], [0.9, 0.55]], &[[0.68, 0.4], [0.92, 0.55], [0.68, 0.7]]], 1.0),
    ('−', &[&[[0.15, 0.55], [0.75, 0.55]]], 0.9),
    ('×', &[&[[0.2, 0.35], [0.65, 0.78]], &[[0.65, 0.35], [0.2, 0.78]]], 0.85),
    ('★', &[&[[0.5, 0.1], [0.62, 0.42], [0.95, 0.42], [0.68, 0.62], [0.78, 0.95], [0.5, 0.74], [0.22, 0.95], [0.32, 0.62], [0.05, 0.42], [0.38, 0.42], [0.5, 0.1]]], 1.0),
    ('↳', &[&[[0.2, 0.2], [0.2, 0.7], [0.8, 0.7]], &[[0.62, 0.55], [0.82, 0.7], [0.62, 0.85]]], 0.9),
];

pub struct Glyphs {
    latin: HashMap<char, Arc<Glyph>>,
    punct: HashMap<char, Arc<Glyph>>,
    kana: HashMap<char, Arc<Glyph>>,
    hanzi_dir: PathBuf,
    hanzi: HashMap<char, Option<Arc<Glyph>>>,
}

impl Glyphs {
    /// `root` is a boardcast checkout with `public/hanzi` generated.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("src/ink/handwritten.json");
        let raw = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let table: HashMap<String, Glyph> = serde_json::from_slice(&raw)?;
        let latin = table
            .into_iter()
            .filter_map(|(k, g)| Some((k.chars().next()?, Arc::new(Glyph { latin: true, ..g }))))
            .collect();
        let raw = std::fs::read(root.join("src/ink/kana.json")).context("reading kana")?;
        let table: HashMap<String, Glyph> = serde_json::from_slice(&raw)?;
        let kana = table.into_iter().filter_map(|(k, g)| Some((k.chars().next()?, Arc::new(g)))).collect();
        let punct = PUNCT
            .iter()
            .map(|(c, strokes, advance)| (*c, Arc::new(Glyph { strokes: strokes.iter().map(|s| s.to_vec()).collect(), advance: *advance, latin: false })))
            .collect();
        Ok(Self { latin, punct, kana, hanzi_dir: root.join("public/hanzi"), hanzi: HashMap::new() })
    }

    /// Characters without data return `None`; the caller leaves a blank advance.
    pub fn get(&mut self, ch: char) -> Option<Arc<Glyph>> {
        if let Some(g) = self.punct.get(&ch).or_else(|| self.kana.get(&ch)) {
            return Some(g.clone());
        }
        // Full-width letters and digits reuse the English strokes.
        let plain = match ch {
            '！'..='～' => char::from_u32(ch as u32 - 0xfee0).unwrap_or(ch),
            _ => ch,
        };
        if let Some(g) = self.latin.get(&plain) {
            return Some(g.clone());
        }
        if let Some(g) = self.hanzi.get(&ch) {
            return g.clone();
        }
        let g = read_hanzi(&self.hanzi_dir, ch).map(Arc::new);
        self.hanzi.insert(ch, g.clone());
        g
    }
}

/// Medians from hanzi-writer: a 1024 square with y up and the baseline at 900.
fn read_hanzi(dir: &Path, ch: char) -> Option<Glyph> {
    let raw = std::fs::read(dir.join(format!("{ch}.json"))).ok()?;
    let medians: Vec<Vec<[f32; 2]>> = serde_json::from_slice(&raw).ok()?;
    let strokes = medians
        .into_iter()
        .map(|s| s.into_iter().map(|p| [p[0] / 1024.0, (900.0 - p[1]) / 1024.0]).collect())
        .collect();
    Some(Glyph { strokes, advance: 1.0, latin: false })
}
