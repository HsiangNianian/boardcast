//! Error rates by each language's own units, ported from eval/asr/score.mjs: characters for Chinese and
//! Japanese (CER), words for English (WER), and both for mixed Chinese and English (MER).

use unicode_normalization::UnicodeNormalization;

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x2FDF | 0x3005 | 0x3007 | 0x3021..=0x3029 | 0x3038..=0x303B | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x323AF)
}

/// The units text is scored in. Punctuation, symbols and spaces never count.
pub fn tokens(text: &str, language: &str) -> Vec<String> {
    let clean: String = text.nfkc().collect::<String>().to_lowercase().chars().filter(|c| !matches!(c, '’' | '\'' | '_')).collect();
    let mut out = vec![];
    match language {
        "en" => {
            let mut word = String::new();
            for c in clean.chars() {
                if c.is_alphanumeric() {
                    word.push(c);
                } else if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            if !word.is_empty() {
                out.push(word);
            }
        }
        "zh-en" => {
            // Each Han character alone, Latin letters and digits as whole words, anything else letter by letter.
            let mut latin = String::new();
            for c in clean.chars() {
                if c.is_ascii_lowercase() || c.is_ascii_digit() {
                    latin.push(c);
                    continue;
                }
                if !latin.is_empty() {
                    out.push(std::mem::take(&mut latin));
                }
                if is_han(c) || c.is_alphanumeric() {
                    out.push(c.to_string());
                }
            }
            if !latin.is_empty() {
                out.push(latin);
            }
        }
        _ => out.extend(clean.chars().filter(|c| c.is_alphanumeric()).map(|c| c.to_string())),
    }
    out
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ErrorRate {
    pub errors: usize,
    pub units: usize,
    /// None without a reference: an empty reference has no rate, rather than a perfect one.
    pub rate: Option<f64>,
}

pub fn error_rate(reference: &str, hypothesis: &str, language: &str) -> ErrorRate {
    let (want, got) = (tokens(reference, language), tokens(hypothesis, language));
    let mut previous: Vec<usize> = (0..=got.len()).collect();
    for (i, w) in want.iter().enumerate() {
        let mut next = vec![i + 1];
        for (j, g) in got.iter().enumerate() {
            next.push((next[j] + 1).min(previous[j + 1] + 1).min(previous[j] + usize::from(w != g)));
        }
        previous = next;
    }
    let errors = previous[got.len()];
    ErrorRate { errors, units: want.len(), rate: (!want.is_empty()).then(|| errors as f64 / want.len() as f64) }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Recall {
    pub hits: Vec<String>,
    pub total: usize,
    pub rate: Option<f64>,
}

/// Which keywords appear as a complete run of tokens: "python" is not found in "Pythonic".
pub fn keyword_recall(keywords: &[String], hypothesis: &str, language: &str) -> Recall {
    let got = tokens(hypothesis, language);
    let hits: Vec<String> = keywords
        .iter()
        .filter(|word| {
            let want = tokens(word, language);
            !want.is_empty() && (0..got.len()).any(|i| want.iter().enumerate().all(|(j, t)| got.get(i + j) == Some(t)))
        })
        .cloned()
        .collect();
    Recall { total: keywords.len(), rate: (!keywords.is_empty()).then(|| hits.len() as f64 / keywords.len() as f64), hits }
}

/// Recognition that went wrong whatever the error rate: nothing heard where someone spoke, or words where nobody did.
pub fn recognition_failures<'a>(samples: impl Iterator<Item = (&'a str, &'a str, &'a str)>) -> Vec<String> {
    samples
        .filter_map(|(id, scoring, hypothesis)| match (scoring == "silence", hypothesis.trim().is_empty()) {
            (true, false) => Some(format!("{id}: speech hallucinated on silence")),
            (false, true) => Some(format!("{id}: no speech recognized")),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_follow_each_language() {
        assert_eq!(tokens("ＡＰＩ、更新。", "ja"), ["a", "p", "i", "更", "新"]);
        assert_eq!(tokens("Don't split P_H_D_", "en"), ["dont", "split", "phd"]);
        assert_eq!(tokens("使用 Python 3。", "zh-en"), ["使", "用", "python", "3"]);
    }

    #[test]
    fn error_rates_count_edits_and_leave_an_empty_reference_unscored() {
        assert_eq!(error_rate("我们测试。", "我们测试", "zh").rate, Some(0.0));
        assert_eq!(error_rate("one two three", "one four three", "en"), ErrorRate { errors: 1, units: 3, rate: Some(1.0 / 3.0) });
        assert_eq!(error_rate("one two three", "one three", "en").errors, 1);
        assert_eq!(error_rate("one", "one two three", "en").rate, Some(2.0));
        assert_eq!(error_rate("", "invented words", "en"), ErrorRate { errors: 2, units: 0, rate: None });
        assert_eq!(error_rate("日本語", "", "ja").rate, Some(1.0));
    }

    #[test]
    fn keywords_match_whole_token_runs() {
        let r = keyword_recall(&["python".into(), "computer languages".into()], "Pythonic computer languages", "zh-en");
        assert_eq!(r.hits, ["computer languages"]);
        assert_eq!(r.rate, Some(0.5));
    }

    #[test]
    fn one_missing_transcript_fails_even_when_others_are_heard() {
        let rows = [("ok", "reference", "hello"), ("quiet", "reference", " "), ("silence", "silence", "")];
        assert_eq!(recognition_failures(rows.into_iter()), ["quiet: no speech recognized"]);
        let rows = [("silence", "silence", "hm")];
        assert_eq!(recognition_failures(rows.into_iter()), ["silence: speech hallucinated on silence"]);
    }
}
