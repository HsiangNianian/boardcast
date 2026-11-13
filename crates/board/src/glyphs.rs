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

/// The stroke data for Chinese and Japanese characters.
#[derive(Clone)]
pub enum HanziSource {
    /// One file holding them all, as an install ships them.
    Pack(PathBuf),
    /// One JSON file per character, as `boardcast-tools fetch glyphs` generates them in a checkout.
    Directory(PathBuf),
    /// A pack already in memory, as a web page carries a few characters of one.
    PackBytes(&'static [u8]),
}

enum Hanzi {
    Pack(Pack),
    Directory(PathBuf),
}

pub struct Glyphs {
    latin: HashMap<char, Arc<Glyph>>,
    cursive: HashMap<char, Arc<Glyph>>,
    punct: HashMap<char, Arc<Glyph>>,
    kana: HashMap<char, Arc<Glyph>>,
    source: Hanzi,
    hanzi: HashMap<char, Option<Arc<Glyph>>>,
    messy: f32,
}

impl Glyphs {
    /// Latin and kana data are built in; only the much larger Chinese data comes from `source`.
    pub fn load(source: HanziSource) -> Result<Self> {
        let table: HashMap<String, Glyph> = serde_json::from_str(include_str!("../../../assets/glyphs/handwritten.json"))?;
        let latin = table
            .into_iter()
            .filter_map(|(k, g)| Some((k.chars().next()?, Arc::new(Glyph { latin: true, ..g }))))
            .collect();
        let table: HashMap<String, Glyph> = serde_json::from_str(include_str!("../../../assets/glyphs/cursive.json"))?;
        let cursive = table
            .into_iter()
            .filter_map(|(k, g)| Some((k.chars().next()?, Arc::new(Glyph { latin: true, ..g }))))
            .collect();
        let table: HashMap<String, Glyph> = serde_json::from_str(include_str!("../../../assets/glyphs/kana.json"))?;
        let kana = table.into_iter().filter_map(|(k, g)| Some((k.chars().next()?, Arc::new(g)))).collect();
        let punct = PUNCT
            .iter()
            .map(|(c, strokes, advance)| (*c, Arc::new(Glyph { strokes: strokes.iter().map(|s| s.to_vec()).collect(), advance: *advance, latin: false })))
            .collect();
        let source = match source {
            HanziSource::Pack(path) => Hanzi::Pack(Pack::read(&path)?),
            HanziSource::Directory(path) => Hanzi::Directory(path),
            HanziSource::PackBytes(bytes) => Hanzi::Pack(Pack::parse(bytes).context("the glyph pack is damaged")?),
        };
        Ok(Self { latin, cursive, punct, kana, source, hanzi: HashMap::new(), messy: 0.0 })
    }

    /// A boardcast checkout with `assets/hanzi` generated.
    pub fn from_checkout(root: &Path) -> Result<Self> {
        Self::load(HanziSource::Directory(root.join("assets/hanzi")))
    }

    /// How messy the hand writing with these is, from 0 (neat, the default) to 1.
    pub fn messy(&self) -> f32 {
        self.messy
    }

    pub fn set_messy(&mut self, messy: f32) {
        self.messy = messy.clamp(0.0, 1.0);
    }

    /// A small English letter joined up, as a messy hand writes it (Hershey script).
    pub fn cursive(&self, ch: char) -> Option<Arc<Glyph>> {
        self.cursive.get(&ch).cloned()
    }

    /// Characters without data return `None`; the caller leaves a blank advance.
    /// A Chinese character's strokes, as a test gives them: CI does not fetch the stroke data.
    #[cfg(test)]
    pub(crate) fn add_hanzi(&mut self, ch: char, strokes: Vec<Stroke>) {
        self.hanzi.insert(ch, Some(Arc::new(Glyph { strokes, advance: 1.0, latin: false })));
    }

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
        let medians = match &self.source {
            Hanzi::Pack(pack) => pack.get(ch),
            Hanzi::Directory(dir) => read_medians(dir, ch),
        };
        let g = medians.map(|strokes| Arc::new(glyph_from(strokes)));
        self.hanzi.insert(ch, g.clone());
        g
    }
}

/// Medians from hanzi-writer: a 1024 square with y up and the baseline at 900.
fn glyph_from(medians: Vec<Vec<[f32; 2]>>) -> Glyph {
    let strokes = medians
        .into_iter()
        .map(|s| s.into_iter().map(|p| [p[0] / 1024.0, (900.0 - p[1]) / 1024.0]).collect())
        .collect();
    Glyph { strokes, advance: 1.0, latin: false }
}

fn read_medians(dir: &Path, ch: char) -> Option<Vec<Vec<[f32; 2]>>> {
    serde_json::from_slice(&std::fs::read(dir.join(format!("{ch}.json"))).ok()?).ok()
}

const PACK_MAGIC: &[u8; 8] = b"BCGLYPH1";

/// Every character in one file: a sorted index followed by the strokes as little-endian numbers.
/// A folder of eleven thousand small files is slow to install and wasteful on disk.
struct Pack {
    index: Vec<(u32, u32)>,
    data: Vec<u8>,
}

impl Pack {
    fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&bytes).with_context(|| format!("{} is damaged", path.display()))
    }

    fn parse(bytes: &[u8]) -> Result<Self> {
        let header = bytes.get(..12).context("too short")?;
        anyhow::ensure!(&header[..8] == PACK_MAGIC, "not a glyph pack");
        let count = u32::from_le_bytes(header[8..12].try_into()?) as usize;
        let table_end = 12 + count * 8;
        let table = bytes.get(12..table_end).context("index cut short")?;
        let index = table.chunks_exact(8).map(|e| (u32::from_le_bytes([e[0], e[1], e[2], e[3]]), u32::from_le_bytes([e[4], e[5], e[6], e[7]]))).collect();
        Ok(Self { index, data: bytes[table_end..].to_vec() })
    }

    fn get(&self, ch: char) -> Option<Vec<Vec<[f32; 2]>>> {
        let at = self.index.binary_search_by_key(&(ch as u32), |e| e.0).ok()?;
        let mut cursor = self.data.get(self.index[at].1 as usize..)?;
        let take_u16 = |c: &mut &[u8]| -> Option<u16> {
            let (head, rest) = c.split_first_chunk::<2>()?;
            *c = rest;
            Some(u16::from_le_bytes(*head))
        };
        let strokes = take_u16(&mut cursor)?;
        let mut out = Vec::with_capacity(strokes as usize);
        for _ in 0..strokes {
            let points = take_u16(&mut cursor)?;
            let mut stroke = Vec::with_capacity(points as usize);
            for _ in 0..points {
                let (head, rest) = cursor.split_first_chunk::<8>()?;
                cursor = rest;
                stroke.push([f32::from_le_bytes([head[0], head[1], head[2], head[3]]), f32::from_le_bytes([head[4], head[5], head[6], head[7]])]);
            }
            out.push(stroke);
        }
        Some(out)
    }
}

/// Build a pack from the folder `boardcast-tools fetch glyphs` generates. Returns how many characters it holds.
pub fn write_pack(dir: &Path, out: &Path) -> Result<usize> {
    write_pack_of(dir, out, None)
}

/// As [`write_pack`], keeping only the characters in `only` when given.
pub fn write_pack_of(dir: &Path, out: &Path, only: Option<&std::collections::HashSet<char>>) -> Result<usize> {
    let mut entries: Vec<(u32, Vec<Vec<[f32; 2]>>)> = vec![];
    for file in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = file?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let mut chars = stem.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else { continue };
        if only.is_some_and(|keep| !keep.contains(&ch)) {
            continue;
        }
        let medians: Vec<Vec<[f32; 2]>> = serde_json::from_slice(&std::fs::read(&path)?).with_context(|| format!("parsing {}", path.display()))?;
        entries.push((ch as u32, medians));
    }
    entries.sort_by_key(|e| e.0);
    let mut data = vec![];
    let mut index = Vec::with_capacity(entries.len() * 8);
    for (ch, medians) in &entries {
        index.extend_from_slice(&ch.to_le_bytes());
        index.extend_from_slice(&u32::try_from(data.len())?.to_le_bytes());
        data.extend_from_slice(&u16::try_from(medians.len())?.to_le_bytes());
        for stroke in medians {
            data.extend_from_slice(&u16::try_from(stroke.len())?.to_le_bytes());
            for p in stroke {
                data.extend_from_slice(&p[0].to_le_bytes());
                data.extend_from_slice(&p[1].to_le_bytes());
            }
        }
    }
    let mut bytes = Vec::with_capacity(12 + index.len() + data.len());
    bytes.extend_from_slice(PACK_MAGIC);
    bytes.extend_from_slice(&u32::try_from(entries.len())?.to_le_bytes());
    bytes.extend_from_slice(&index);
    bytes.extend_from_slice(&data);
    std::fs::write(out, bytes).with_context(|| format!("writing {}", out.display()))?;
    Ok(entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_returns_exactly_what_went_in_and_rejects_damage() {
        let dir = std::env::temp_dir().join(format!("boardcast-glyphs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("甲.json"), "[[[108,472],[180.5,470]],[[261,814]]]").unwrap();
        std::fs::write(dir.join("乙.json"), "[[[1,-18]]]").unwrap();
        std::fs::write(dir.join("ARPHICPL.TXT"), "not a glyph").unwrap();
        std::fs::write(dir.join("not-one-character.json"), "[]").unwrap();
        let out = dir.join("glyphs.pack");
        assert_eq!(write_pack(&dir, &out).unwrap(), 2);
        let pack = Pack::read(&out).unwrap();
        assert_eq!(pack.get('甲').unwrap(), read_medians(&dir, '甲').unwrap());
        assert_eq!(pack.get('乙').unwrap(), [[[1.0, -18.0]]]);
        assert_eq!(pack.get('丙'), None);
        let mut glyphs = Glyphs::load(HanziSource::Pack(out.clone())).unwrap();
        assert_eq!(glyphs.get('甲').unwrap().strokes.len(), 2);
        assert!(glyphs.get('A').unwrap().latin, "Latin is built in");
        assert!(glyphs.get('あ').is_some(), "kana is built in");
        assert!(Pack::parse(&[b"BCGLYPH1".as_slice(), &[0xff; 4]].concat()).is_err());
        assert!(Pack::parse(b"nonsense").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The English strokes run as a hand writes them (scripts/excalifont-strokes.py puts them so): a line
    /// across goes left to right, a line down goes top to bottom, and a letter is not left in pieces.
    #[test]
    fn english_strokes_run_the_way_a_hand_writes() {
        let table: HashMap<String, Glyph> = serde_json::from_str(include_str!("../../../assets/glyphs/handwritten.json")).unwrap();
        let length = |s: &Stroke| s.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f32>();
        let mut wrong = vec![];
        for (ch, g) in &table {
            for s in g.strokes.iter().filter(|s| s.len() >= 2 && length(s) >= 0.1) {
                let (a, b) = (s[0], s[s.len() - 1]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                if dx.hypot(dy) < 0.2 * length(s) {
                    continue; // a loop
                }
                if (dx.abs() > 2.0 * dy.abs() && dx < 0.0) || (dy.abs() > 2.0 * dx.abs() && dy < 0.0) {
                    wrong.push(ch.clone());
                }
            }
        }
        assert!(wrong.is_empty(), "strokes running backwards in {wrong:?}");
        let count = |ch: &str| table[ch].strokes.len();
        for (ch, n) in [("A", 3), ("E", 3), ("H", 3), ("O", 1), ("X", 2), ("a", 1), ("e", 1), ("i", 2), ("m", 3), ("x", 2)] {
            assert_eq!(count(ch), n, "strokes in {ch}");
        }
        // The dot of an i is written last.
        assert!(table["i"].strokes[1][0][1] < table["i"].strokes[0][0][1]);
    }
}
