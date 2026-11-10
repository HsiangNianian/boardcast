//! Words as they sound. Recognition writes what it heard, which for a name is often the wrong
//! characters with the right sound: 鉴权 heard as 健全, OpenAI as "open ai". A glossary says how names
//! are written, and lines are corrected to it; the board's own labels give the model a hint when a line
//! says one of them in other characters.
//!
//! Chinese is compared by pinyin without tones, character by character, any reading of a character
//! counting. English words are compared by their sounds in the CMU Pronouncing Dictionary, without
//! stress, so "cash" is cache and "cue" is queue; and without case or spaces, so "open ai" is OpenAI and
//! "A P I" is API. Japanese is compared by its readings, kana as katakana and each kanji by its on and kun
//! readings from KANJIDIC2, allowing a later kanji's first sound to voice and a final one to double: 構成
//! and 公正 both read コウセイ, 認証 is にんしょう. That covers the Sino-Japanese words names mostly are;
//! a word read irregularly needs an alias. It is only tried on lines with kana, as a Chinese line's
//! characters have Japanese readings too, and Chinese is compared by pinyin.

use pinyin::ToPinyinMulti;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::OnceLock;

/// Every English word's ways of being said, as sounds without stress: "cache" → ["K AE SH"].
fn cmu() -> &'static HashMap<String, Vec<String>> {
    static CMU: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    CMU.get_or_init(|| {
        let mut text = String::new();
        let packed: &[u8] = include_bytes!("../../../assets/dictionaries/cmudict.dict.gz");
        if flate2::read::GzDecoder::new(packed).read_to_string(&mut text).is_err() {
            return HashMap::new();
        }
        let mut words: HashMap<String, Vec<String>> = HashMap::new();
        for line in text.lines() {
            let Some((word, sounds)) = line.split_once(' ') else { continue };
            // Variants are written word(2); a trailing comment starts with #.
            let word = word.split('(').next().unwrap_or(word);
            let sounds: Vec<&str> = sounds.split('#').next().unwrap_or("").split_whitespace().map(|p| p.trim_end_matches(|c: char| c.is_ascii_digit())).collect();
            words.entry(word.to_string()).or_default().push(sounds.join(" "));
        }
        words
    })
}

/// Load the dictionaries now, away from the window, rather than on the first line that needs one.
pub fn warm() {
    cmu();
    kanji();
}

/// Each kanji's readings, in katakana.
fn kanji() -> &'static HashMap<char, Vec<String>> {
    static KANJI: OnceLock<HashMap<char, Vec<String>>> = OnceLock::new();
    KANJI.get_or_init(|| {
        let mut text = String::new();
        let packed: &[u8] = include_bytes!("../../../assets/dictionaries/kanji-readings.tsv.gz");
        if flate2::read::GzDecoder::new(packed).read_to_string(&mut text).is_err() {
            return HashMap::new();
        }
        text.lines()
            .filter_map(|l| {
                let (k, r) = l.split_once('\t')?;
                Some((k.chars().next()?, r.split(',').map(str::to_string).collect()))
            })
            .collect()
    })
}

/// The voiced form of a sound, as the second part of a compound often takes: コ → ゴ, ホ → ボ.
fn voiced(c: char) -> Option<char> {
    let n = c as u32;
    // カ to ト and ハ to ホ, where the voiced form comes next in the table.
    let plain = matches!(n, 0x30AB..=0x30C8 if (n - 0x30AB) % 2 == 0) || matches!(n, 0x30CF..=0x30DB if (n - 0x30CF) % 3 == 0);
    plain.then(|| char::from_u32(n + 1)).flatten()
}

/// How a Japanese word can be read, in katakana. Empty if a character has no reading here.
fn read(word: &str) -> HashSet<String> {
    let mut ways: HashSet<String> = [String::new()].into();
    for (i, c) in word.chars().enumerate() {
        let parts: Vec<String> = if matches!(c as u32, 0x3041..=0x30FF) {
            vec![katakana(&c.to_string())]
        } else if let Some(readings) = kanji().get(&c) {
            let mut parts = readings.clone();
            for r in readings {
                let mut chars: Vec<char> = r.chars().collect();
                if i > 0 {
                    if let Some(v) = chars.first().copied().and_then(voiced) {
                        let mut voiced_form = chars.clone();
                        voiced_form[0] = v;
                        parts.push(voiced_form.into_iter().collect());
                    }
                }
                if matches!(chars.last(), Some('ク' | 'キ' | 'ツ' | 'チ')) {
                    chars.pop();
                    chars.push('ッ');
                    parts.push(chars.into_iter().collect());
                }
            }
            parts
        } else {
            return HashSet::new();
        };
        ways = ways.iter().flat_map(|w| parts.iter().map(move |p| format!("{w}{p}"))).take(256).collect();
    }
    if ways.contains("") { HashSet::new() } else { ways }
}

fn japanese(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| is_han(c) || matches!(c as u32, 0x3041..=0x30FF))
}

/// Whether two Japanese words, written differently, can be read the same.
pub fn read_alike(a: &str, b: &str) -> bool {
    japanese(a) && japanese(b) && a != b && katakana(a) != katakana(b) && !read(a).is_disjoint(&read(b))
}

/// How a run of English words can be said, each way as one string of sounds. Empty if a word is unknown.
fn said(words: &str) -> HashSet<String> {
    let mut ways: HashSet<String> = [String::new()].into();
    for word in words.split(|c: char| !c.is_ascii_alphanumeric() && c != '\'').filter(|w| !w.is_empty()) {
        let Some(sounds) = cmu().get(&word.to_ascii_lowercase()) else { return HashSet::new() };
        // A long phrase with many variants per word is cut short rather than grown without end.
        ways = ways.iter().flat_map(|w| sounds.iter().map(move |s| if w.is_empty() { s.clone() } else { format!("{w} {s}") })).take(64).collect();
    }
    if ways.contains("") { HashSet::new() } else { ways }
}

/// Whether two English words or phrases, spelled differently, are said the same.
pub fn said_alike(a: &str, b: &str) -> bool {
    let latin = |s: &str| s.chars().any(|c| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii());
    latin(a) && latin(b) && squeeze(a) != squeeze(b) && !said(a).is_disjoint(&said(b))
}

/// Runs of one to three words in Latin text, with where each lies.
fn word_runs(text: &str, longest: usize) -> Vec<(usize, usize)> {
    let mut words = vec![];
    let mut start = None;
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (c.is_ascii_alphanumeric() || c == '\'', start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    let mut runs = vec![];
    for n in 1..=longest {
        for w in words.windows(n) {
            if text[w[0].0..w[n - 1].1].chars().all(|c| c.is_ascii()) {
                runs.push((w[0].0, w[n - 1].1));
            }
        }
    }
    runs
}

/// How a set of names is written, as entered: one per line, `Name` or `Name = how it is heard, …`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glossary {
    pub terms: Vec<Term>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub text: String,
    /// Other ways it comes out of recognition.
    pub heard: Vec<String>,
}

impl Glossary {
    pub fn parse(text: &str) -> Self {
        // One name per line, or several on a line apart by semicolons, as a one-line field takes them.
        let terms = text
            .split(['\n', ';', '；'])
            .filter_map(|line| {
                let line = line.trim();
                let (name, heard) = line.split_once(['=', '＝']).map_or((line, ""), |(n, h)| (n.trim(), h));
                (!name.is_empty()).then(|| Term {
                    text: name.to_string(),
                    heard: heard.split([',', '，', '、']).map(str::trim).filter(|h| !h.is_empty() && *h != name).map(str::to_string).collect(),
                })
            })
            .collect();
        Self { terms }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// A line with its names written the glossary's way.
    pub fn correct(&self, text: &str) -> String {
        let mut out = text.to_string();
        for term in &self.terms {
            for heard in &term.heard {
                out = replace_words(&out, heard, &term.text);
            }
            out = replace_spelled(&out, &term.text);
            if han(&term.text) {
                out = replace_homophones(&out, &term.text, &self.terms);
            }
            if kana(&term.text) {
                out = replace_kana(&out, &term.text);
            }
            if japanese(&term.text) && term.text.chars().any(is_han) && has_kana(&out) {
                out = replace_read(&out, &term.text, &self.terms);
            }
            if term.text.chars().all(|c| c.is_ascii()) {
                out = replace_said(&out, &term.text, &self.terms);
            }
        }
        out
    }
}

fn han(s: &str) -> bool {
    let n = s.chars().count();
    n >= 2 && s.chars().all(is_han)
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn kana(s: &str) -> bool {
    s.chars().count() >= 2 && s.chars().all(|c| matches!(c as u32, 0x3040..=0x30FF))
}

/// Katakana for hiragana, so either spelling of a word compares the same.
fn katakana(s: &str) -> String {
    s.chars().map(|c| if matches!(c as u32, 0x3041..=0x3096) { char::from_u32(c as u32 + 0x60).unwrap_or(c) } else { c }).collect()
}

/// Every toneless reading of a character.
fn readings(c: char) -> HashSet<&'static str> {
    c.to_pinyin_multi().map_or_else(HashSet::new, |m| m.into_iter().map(|p| p.plain()).collect())
}

/// Whether two runs of Chinese characters, of the same length, can be read alike.
pub fn sound_alike(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| x == y || !readings(*x).is_disjoint(&readings(*y)))
}

/// Latin letters and digits, lowercase, nothing between them: "Open AI", "open-ai" and "OpenAI" are one.
fn squeeze(s: &str) -> String {
    s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// Replace whole words of `from` with `to`: Latin text only at word edges, other scripts anywhere.
fn replace_words(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let latin = from.chars().all(|c| c.is_ascii());
    let (lower, needle) = if latin { (text.to_ascii_lowercase(), from.to_ascii_lowercase()) } else { (text.to_string(), from.to_string()) };
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut at = 0;
    while let Some(found) = lower[at..].find(&needle) {
        let start = at + found;
        let end = start + needle.len();
        let edge = |c: Option<char>| c.is_none_or(|c| !c.is_ascii_alphanumeric());
        if !latin || (edge(lower[..start].chars().last()) && edge(lower[end..].chars().next())) {
            out.push_str(&text[last..start]);
            out.push_str(to);
            last = end;
        }
        at = end;
    }
    out.push_str(&text[last..]);
    out
}

/// A Latin name heard in pieces: "open ai" or "A P I" for OpenAI or API, up to four words.
fn replace_spelled(text: &str, name: &str) -> String {
    let key = squeeze(name);
    if key.len() < 2 || !name.chars().all(|c| c.is_ascii()) {
        return text.to_string();
    }
    let words: Vec<(usize, usize)> = {
        let mut spans = vec![];
        let mut start = None;
        for (i, c) in text.char_indices() {
            match (c.is_ascii_alphanumeric(), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    spans.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            spans.push((s, text.len()));
        }
        spans
    };
    for n in (2..=4).rev() {
        for w in words.windows(n) {
            let (start, end) = (w[0].0, w[n - 1].1);
            // Only spaces, dots or hyphens between the pieces.
            let gaps = text[start..end].chars().filter(|c| !c.is_ascii_alphanumeric()).all(|c| matches!(c, ' ' | '.' | '-'));
            if gaps && squeeze(&text[start..end]) == key {
                return format!("{}{name}{}", &text[..start], replace_spelled(&text[end..], name));
            }
        }
    }
    text.to_string()
}

/// Runs of Chinese characters that read like `name` become `name`, unless the run is another name.
fn replace_homophones(text: &str, name: &str, terms: &[Term]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let size = name.chars().count();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let window: String = chars[i..(i + size).min(chars.len())].iter().collect();
        let fits = i + size <= chars.len() && window.chars().all(is_han);
        if fits && window != name && !terms.iter().any(|t| t.text == window) && sound_alike(&window, name) {
            out.push_str(name);
            i += size;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// English words said like `name` become `name`, unless they are another name already.
fn replace_said(text: &str, name: &str, terms: &[Term]) -> String {
    let size = name.split_whitespace().count();
    let found = word_runs(text, size).into_iter().filter(|&(a, b)| text[a..b].split_whitespace().count() == size).find(|&(a, b)| {
        let run = &text[a..b];
        !terms.iter().any(|t| squeeze(&t.text) == squeeze(run)) && said_alike(run, name)
    });
    match found {
        Some((a, b)) => format!("{}{name}{}", &text[..a], replace_said(&text[b..], name, terms)),
        None => text.to_string(),
    }
}

fn has_kana(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF))
}

/// Japanese runs read like `name` become `name`, unless they are another name already. The shortest run
/// that fits is taken, from the left.
fn replace_read(text: &str, name: &str, terms: &[Term]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let wanted = read(name);
    for start in 0..chars.len() {
        for end in start + 1..=(start + 10).min(chars.len()) {
            let run: String = chars[start..end].iter().collect();
            if !japanese(&run) || run == name || terms.iter().any(|t| t.text == run) {
                continue;
            }
            if !read(&run).is_disjoint(&wanted) {
                let before: String = chars[..start].iter().collect();
                let after: String = chars[end..].iter().collect();
                return format!("{before}{name}{}", replace_read(&after, name, terms));
            }
        }
    }
    text.to_string()
}

fn replace_kana(text: &str, name: &str) -> String {
    let (key, chars): (Vec<char>, Vec<char>) = (katakana(name).chars().collect(), text.chars().collect());
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let window: String = chars[i..(i + key.len()).min(chars.len())].iter().collect();
        if i + key.len() <= chars.len() && katakana(&window).chars().eq(key.iter().copied()) {
            out.push_str(name);
            i += key.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Words in a line that read like one of the board's labels but are written otherwise, with the label:
/// a hint for the model, which then knows "健全" may mean the item 鉴权. Only a reading that fits one
/// label is given, as one that fits two says nothing.
pub fn board_hints(text: &str, labels: &[&str]) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<(String, String)> = vec![];
    let lower = text.to_ascii_lowercase();
    for label in labels.iter().filter(|l| l.chars().all(|c| c.is_ascii()) && !lower.contains(&l.to_ascii_lowercase())) {
        let size = label.split_whitespace().count().max(1);
        for (a, b) in word_runs(text, size) {
            let run = &text[a..b];
            if run.split_whitespace().count() == size && said_alike(run, label) && !labels.iter().any(|l| squeeze(l) == squeeze(run)) {
                let others = labels.iter().filter(|l| *l != label && said_alike(run, l)).count();
                if others == 0 && !out.iter().any(|(w, _)| w == run) {
                    out.push((run.to_string(), (*label).to_string()));
                }
            }
        }
    }
    if has_kana(text) {
        let chars: Vec<char> = text.chars().collect();
        for label in labels.iter().filter(|l| japanese(l) && l.chars().any(is_han) && !text.contains(*l)) {
            let wanted = read(label);
            for start in 0..chars.len() {
                for end in start + 2..=(start + 10).min(chars.len()) {
                    let run: String = chars[start..end].iter().collect();
                    if !japanese(&run) || labels.contains(&run.as_str()) || read(&run).is_disjoint(&wanted) {
                        continue;
                    }
                    let others = labels.iter().filter(|l| *l != label && read_alike(&run, l)).count();
                    if others == 0 && !out.iter().any(|(w, _)| *w == run) {
                        out.push((run, (*label).to_string()));
                    }
                }
            }
        }
        return out;
    }
    for label in labels.iter().filter(|l| han(l) && !text.contains(*l)) {
        let size = label.chars().count();
        for start in 0..chars.len().saturating_sub(size - 1) {
            let window: String = chars[start..start + size].iter().collect();
            if window.chars().all(is_han) && sound_alike(&window, label) && !labels.contains(&window.as_str()) {
                let others = labels.iter().filter(|l| *l != label && sound_alike(&window, l)).count();
                if others == 0 && !out.iter().any(|(w, _)| *w == window) {
                    out.push((window, (*label).to_string()));
                }
            }
        }
    }
    out
}

/// What the board model is told besides the line: how names are written, and which words may be labels.
pub fn note(glossary: &Glossary, text: &str, labels: &[&str], english: bool) -> Option<String> {
    let mut parts = vec![];
    if !glossary.is_empty() {
        let names: Vec<&str> = glossary.terms.iter().map(|t| t.text.as_str()).collect();
        parts.push(if english { format!("Glossary, write these names exactly so: {}", names.join(", ")) } else { format!("术语表，名字照这样写：{}", names.join("、")) });
    }
    for (heard, label) in board_hints(text, labels) {
        parts.push(if english { format!("\"{heard}\" in the line may be the board's \"{label}\".") } else { format!("这句里的「{heard}」可能是板上的「{label}」。") });
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// Which label a word in `line` means: the label itself, a glossary spelling of it, or the only label
/// that reads like it; among several that read alike, the one pointed at. None when it can't be told.
pub fn resolve(word: &str, line: &str, labels: &[&str], glossary: &Glossary, pointed: &[usize]) -> Option<usize> {
    // Japanese when the line has kana; otherwise Chinese characters go by pinyin.
    let japanese_case = has_kana(line) || labels.iter().any(|l| has_kana(l));
    if let Some(i) = labels.iter().position(|l| *l == word) {
        return Some(i);
    }
    let written = glossary.correct(word);
    if let Some(i) = labels.iter().position(|l| *l == written || squeeze(l) == squeeze(&written) && !squeeze(l).is_empty()) {
        return Some(i);
    }
    let alike: Vec<usize> = labels
        .iter()
        .enumerate()
        .filter(|(_, l)| (japanese_case && read_alike(word, l)) || (!japanese_case && han(l) && han(word) && sound_alike(word, l)) || (kana(word) && katakana(word) == katakana(l)) || said_alike(word, l))
        .map(|(i, _)| i)
        .collect();
    match alike.as_slice() {
        [one] => Some(*one),
        many if !many.is_empty() => {
            let chosen: Vec<&usize> = many.iter().filter(|i| pointed.contains(i)).collect();
            (chosen.len() == 1).then(|| *chosen[0])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glossary_corrects_how_names_come_out() {
        let g = Glossary::parse("鉴权\nOpenAI\nAPI = A P I, エーピーアイ\nKryptonix = cripto nicks\nボードキャスト");
        assert_eq!(g.terms[2].heard, ["A P I", "エーピーアイ"]);
        assert_eq!(g.correct("用户先过健全再调接口"), "用户先过鉴权再调接口");
        assert_eq!(g.correct("we call open ai from the server"), "we call OpenAI from the server");
        assert_eq!(g.correct("circle A P I please"), "circle API please");
        assert_eq!(g.correct("エーピーアイに丸をつけて"), "APIに丸をつけて");
        assert_eq!(g.correct("circle cripto nicks"), "circle Kryptonix");
        assert_eq!(g.correct("ぼーどきゃすとで書く"), "ボードキャストで書く");
        // Whole words only, and nothing that does not read the same.
        assert_eq!(g.correct("the openair market"), "the openair market");
        assert_eq!(g.correct("系统很健康"), "系统很健康");
        let cache = Glossary::parse("cache; queue");
        assert_eq!(cache.correct("clear the cash and the cue"), "clear the cache and the queue");
        assert_eq!(cache.correct("cashier"), "cashier", "a word of its own");
        let ja = Glossary::parse("構成; 認証");
        assert_eq!(ja.correct("こうせいを見直して、にんしょうも確認"), "構成を見直して、認証も確認");
    }

    #[test]
    fn a_line_saying_a_label_in_other_characters_gets_a_hint() {
        assert_eq!(board_hints("把健全圈起来", &["鉴权", "缓存"]), [("健全".to_string(), "鉴权".to_string())]);
        assert!(board_hints("把鉴权圈起来", &["鉴权", "缓存"]).is_empty(), "said as written");
        assert!(board_hints("把建全圈起来", &["鉴权", "健全"]).is_empty(), "two labels read alike");
        let note = note(&Glossary::default(), "给换存加下划线", &["缓存"], false).unwrap();
        assert!(note.contains("「换存」可能是板上的「缓存」"), "{note}");
        assert_eq!(super::note(&Glossary::default(), "加一个支付服务", &["缓存"], false), None, "nothing to say");
        assert_eq!(board_hints("circle the service cash", &["service cache", "API"]), [("service cash".to_string(), "service cache".to_string())]);
        assert!(board_hints("circle cite", &["site", "sight"]).is_empty(), "two labels said alike");
        assert_eq!(board_hints("公正に下線を引いて", &["構成", "認証"]), [("公正".to_string(), "構成".to_string())]);
        assert_eq!(board_hints("にんしょうに丸をつけて", &["構成", "認証"]), [("にんしょう".to_string(), "認証".to_string())]);
        assert!(board_hints("公正一下格式", &["構成"]).is_empty(), "a Chinese line is not read as Japanese");
    }

    /// The pronunciation cases kept from the earlier app, for what this matching covers. English and
    /// Japanese homophones of different spelling need a pronouncing dictionary, and are listed apart.
    #[test]
    fn the_kept_pronunciation_cases_resolve() {
        let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!("../../../eval/phonetic-cases.json")).unwrap();
        let needs_dictionary: [&str; 0] = [];
        let (mut checked, mut skipped, mut wrong) = (0, 0, vec![]);
        for case in &cases {
            let id = case["id"].as_str().unwrap();
            if needs_dictionary.contains(&id) {
                skipped += 1;
                continue;
            }
            let labels: Vec<&str> = case["labels"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
            let glossary = Glossary::parse(case["glossary"].as_str().unwrap_or(""));
            let pointed: Vec<usize> = case["pointing"].as_array().map_or(vec![], |p| p.iter().map(|i| i.as_u64().unwrap() as usize - 1).collect());
            for (target, want) in case["targets"].as_array().unwrap().iter().zip(case["resolved"].as_array().unwrap()) {
                let got = resolve(target.as_str().unwrap(), case["text"].as_str().unwrap(), &labels, &glossary, &pointed).map(|i| labels[i]);
                if got != want.as_str() {
                    wrong.push(format!("{id}: {target} → {got:?}, want {want}"));
                }
            }
            checked += 1;
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
        assert!(checked >= 20, "{checked} checked, {skipped} need a dictionary");
    }
}
