//! Ranking passages against a query: the cosine of their vectors, plus a share for the query's words
//! found in the passage, so a name or a number that means little to the model still finds its place.

use crate::chunk::Chunk;
use crate::index::Index;

pub struct Hit<'a> {
    pub meeting: &'a str,
    pub chunk: &'a Chunk,
    pub score: f32,
    /// The cosine alone, and the share of the query's words found, for whoever wants to show or tune them.
    pub cosine: f32,
    pub words: f32,
}

/// The query's words: Latin and digit runs of two or more, lowercased, and overlapping pairs of Chinese or
/// Japanese characters, as these are written without spaces.
pub fn words(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut latin = String::new();
    let mut cjk: Vec<char> = vec![];
    let flush_latin = |latin: &mut String, out: &mut Vec<String>| {
        if latin.chars().count() >= 2 {
            out.push(latin.to_lowercase());
        }
        latin.clear();
    };
    let flush_cjk = |cjk: &mut Vec<char>, out: &mut Vec<String>| {
        if cjk.len() == 1 {
            out.push(cjk[0].to_string());
        }
        for pair in cjk.windows(2) {
            out.push(pair.iter().collect());
        }
        cjk.clear();
    };
    for c in text.chars() {
        let wide = matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff);
        if wide {
            flush_latin(&mut latin, &mut out);
            cjk.push(c);
        } else if c.is_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            latin.push(c);
        } else {
            flush_latin(&mut latin, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_latin(&mut latin, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Every passage of every index, best first, at most `limit`. `weight` is how much a full match of the
/// query's words adds to the cosine.
pub fn rank<'a>(indexes: &'a [(String, Index)], query: &[f32], text: &str, weight: f32, limit: usize) -> Vec<Hit<'a>> {
    let wanted = words(text);
    let mut hits: Vec<Hit> = vec![];
    for (meeting, index) in indexes {
        if index.dims != query.len() {
            continue;
        }
        for (i, chunk) in index.chunks.iter().enumerate() {
            let cosine: f32 = index.vector(i).iter().zip(query).map(|(a, b)| a * b).sum();
            let words = if wanted.is_empty() {
                0.0
            } else {
                let lower = chunk.text.to_lowercase();
                wanted.iter().filter(|w| lower.contains(w.as_str())).count() as f32 / wanted.len() as f32
            };
            hits.push(Hit { meeting, chunk, score: cosine + weight * words, cosine, words });
        }
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Source;

    #[test]
    fn words_are_latin_runs_and_character_pairs() {
        assert_eq!(words("Ship 上线时间 v2!"), vec!["ship", "v2", "上线", "时间", "线时"]);
    }

    #[test]
    fn shared_words_lift_a_passage_the_vectors_tie() {
        let chunk = |t: &str| Chunk { source: Source::Transcript { first: 0, last: 0 }, text: t.into() };
        let index = Index::new("f".into(), 2, vec![chunk("canary at ten percent"), chunk("lunch plans")], vec![vec![1.0, 0.0], vec![1.0, 0.0]]);
        let indexes = vec![("m".to_string(), index)];
        let hits = rank(&indexes, &[1.0, 0.0], "canary", 0.2, 2);
        assert_eq!(hits[0].chunk.text, "canary at ten percent");
        assert!(hits[0].score > hits[1].score);
    }
}
