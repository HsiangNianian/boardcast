//! Interface text, in Fluent (https://projectfluent.org): one file of messages for each language in
//! `crates/ui/i18n/`, all sharing the same message IDs. English is the source, and the fallback for
//! anything a language lacks. Code names a message by its ID: `t("id")` for plain text, `ta!("id", n = 3)`
//! when it takes values, which the translation places, inflects and pluralizes as its language needs.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// Every language the interface speaks: its code, as Settings keeps it, and its name in itself.
pub const LANGUAGES: [(&str, &str); 11] = [
    ("en", "English"),
    ("zh-Hans", "简体中文"),
    ("zh-Hant", "繁體中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("it", "Italiano"),
];

const SOURCES: [&str; 11] = [
    include_str!("../i18n/en.ftl"),
    include_str!("../i18n/zh-Hans.ftl"),
    include_str!("../i18n/zh-Hant.ftl"),
    include_str!("../i18n/ja.ftl"),
    include_str!("../i18n/ko.ftl"),
    include_str!("../i18n/es.ftl"),
    include_str!("../i18n/fr.ftl"),
    include_str!("../i18n/de.ftl"),
    include_str!("../i18n/pt.ftl"),
    include_str!("../i18n/ru.ftl"),
    include_str!("../i18n/it.ftl"),
];

const ENGLISH: usize = 0;
/// The choice that follows the system's language.
const SYSTEM: usize = usize::MAX;

static CHOICE: AtomicUsize = AtomicUsize::new(ENGLISH);

/// The language the system prefers, of those there are, or English. A Mac app opened from the Finder has no
/// `LANG`, so this asks the platform rather than the environment.
fn system() -> usize {
    static FOUND: OnceLock<usize> = OnceLock::new();
    *FOUND.get_or_init(|| sys_locale::get_locale().map_or(ENGLISH, |locale| best(&locale)))
}

/// The closest language to a locale such as `zh-TW`, `ja_JP.UTF-8` or `pt-BR`.
fn best(locale: &str) -> usize {
    let locale = locale.replace('_', "-").to_ascii_lowercase();
    let primary = locale.split(['-', '.']).next().unwrap_or_default();
    let code = match primary {
        // Traditional script, or the places that write it.
        "zh" if ["hant", "tw", "hk", "mo"].iter().any(|t| locale.split(['-', '.']).any(|p| p == *t)) => "zh-Hant",
        "zh" => "zh-Hans",
        other => other,
    };
    LANGUAGES.iter().position(|(c, _)| c.eq_ignore_ascii_case(code)).unwrap_or(ENGLISH)
}

fn current() -> usize {
    match CHOICE.load(Ordering::Relaxed) {
        SYSTEM => system(),
        chosen => chosen,
    }
}

/// The code of the language shown now.
pub fn code() -> &'static str {
    LANGUAGES[current()].0
}

/// Whether the interface is in Chinese, for what is written in English or Chinese only.
pub fn chinese() -> bool {
    code().starts_with("zh")
}

/// Use a language by its code, or follow the system for "auto". "zh" is what earlier versions kept for
/// Simplified Chinese. Text drawn from then on follows it.
pub fn choose(language: &str) {
    let choice = match language {
        "auto" => SYSTEM,
        "zh" => 1,
        code => LANGUAGES.iter().position(|(c, _)| *c == code).unwrap_or(ENGLISH),
    };
    CHOICE.store(choice, Ordering::Relaxed);
}

fn bundles() -> &'static [FluentBundle<FluentResource>] {
    static BUNDLES: OnceLock<Vec<FluentBundle<FluentResource>>> = OnceLock::new();
    BUNDLES.get_or_init(|| {
        LANGUAGES
            .iter()
            .zip(SOURCES)
            .map(|((code, _), source)| {
                let id: unic_langid::LanguageIdentifier = code.parse().unwrap_or_default();
                let mut bundle = FluentBundle::new_concurrent(vec![id]);
                // The marks that keep right-to-left text apart are not needed by these languages, and the
                // interface's fonts would show them.
                bundle.set_use_isolating(false);
                let resource = FluentResource::try_new(source.to_string()).unwrap_or_else(|(resource, errors)| {
                    eprintln!("{code}.ftl: {errors:?}");
                    resource
                });
                if let Err(errors) = bundle.add_resource(resource) {
                    eprintln!("{code}.ftl: {errors:?}");
                }
                bundle
            })
            .collect()
    })
}

/// A message in one language, if it has it.
fn format(language: usize, id: &str, args: Option<&FluentArgs>) -> Option<String> {
    let bundle = &bundles()[language];
    let pattern = bundle.get_message(id)?.value()?;
    let mut errors = vec![];
    let text = bundle.format_pattern(pattern, args, &mut errors).into_owned();
    if !errors.is_empty() {
        eprintln!("{}: {id}: {errors:?}", LANGUAGES[language].0);
    }
    Some(text)
}

/// A message in the language shown, or in English, or its ID if neither has it.
fn message(id: &str, args: Option<&FluentArgs>) -> String {
    message_in(current(), id, args)
}

fn message_in(language: usize, id: &str, args: Option<&FluentArgs>) -> String {
    format(language, id, args).or_else(|| (language != ENGLISH).then(|| format(ENGLISH, id, args)).flatten()).unwrap_or_else(|| id.to_string())
}

/// A message that takes no values. Each is worked out once for each language, so the interface can hold it
/// as long as it likes.
pub fn t(id: &str) -> &'static str {
    static MADE: OnceLock<Mutex<HashMap<(usize, String), &'static str>>> = OnceLock::new();
    let key = (current(), id.to_string());
    let mut made = MADE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(text) = made.get(&key) {
        return text;
    }
    let text: &'static str = Box::leak(message(id, None).into_boxed_str());
    made.insert(key, text);
    text
}

/// A message with values; `ta!` builds the values.
pub fn t_args(id: &str, args: &FluentArgs) -> String {
    message(id, Some(args))
}

/// A message with values by name: `ta!("saved-to", path = shown)`. Numbers stay numbers, so a translation
/// can choose its plural.
#[macro_export]
macro_rules! ta {
    ($id:expr $(, $name:ident = $value:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut args = fluent_bundle::FluentArgs::new();
        $(args.set(stringify!($name), $value);)*
        $crate::i18n::t_args($id, &args)
    }};
}

/// A note about a change made by hand, kept in the transcript: its text in English, which is what the meeting
/// keeps as its text and what a model reads, and the message that shows it in the language of whoever looks.
/// Built with `note!`.
pub fn note(id: &str, args: serde_json::Map<String, serde_json::Value>) -> alcyo_meetings::Note {
    let text = format(ENGLISH, id, Some(&fluent_args(&args))).unwrap_or_else(|| id.to_string());
    alcyo_meetings::Note { text, message: serde_json::json!({ "id": id, "args": args }) }
}

/// Values for a message from what a note keeps: numbers stay numbers, so a translation can choose its plural,
/// and a mark stays its key, so each language words each mark its own way.
fn fluent_args(args: &serde_json::Map<String, serde_json::Value>) -> FluentArgs<'static> {
    let mut out = FluentArgs::new();
    for (name, value) in args {
        match value {
            serde_json::Value::Number(n) => out.set(name.clone(), n.as_f64().unwrap_or_default()),
            serde_json::Value::String(text) => out.set(name.clone(), text.clone()),
            other => out.set(name.clone(), other.to_string()),
        }
    }
    out
}

/// A note with values by name: `note!("note-board-forked", to = 3, when = "0:42")`.
#[macro_export]
macro_rules! note {
    ($id:expr $(, $name:ident = $value:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut args = serde_json::Map::new();
        $(args.insert(stringify!($name).to_string(), serde_json::json!($value));)*
        $crate::i18n::note($id, args)
    }};
}

/// A transcript line as the interface shows it: a note in the language shown now, whatever language it was
/// written in, or what was said, as it was said.
pub fn line_text(line: &serde_json::Value) -> String {
    line_text_in(current(), line)
}

fn line_text_in(language: usize, line: &serde_json::Value) -> String {
    let text = line["text"].as_str().unwrap_or_default();
    if !line["manual"].as_bool().unwrap_or(false) {
        return text.to_string();
    }
    let (id, args) = match line["note"]["id"].as_str() {
        Some(id) => (id.to_string(), line["note"]["args"].as_object().cloned().unwrap_or_default()),
        None => match recognize(text) {
            Some(found) => found,
            None => return text.to_string(),
        },
    };
    format(language, &id, Some(&fluent_args(&args))).or_else(|| format(ENGLISH, &id, Some(&fluent_args(&args)))).unwrap_or_else(|| text.to_string())
}

/// The notes a meeting can hold, so ones written as plain text by earlier versions are read again.
const NOTES: [&str; 8] = ["note-board-forked", "note-new-board", "note-moved-to-new-board", "note-imported-image", "note-imported-image-capped", "note-imported-pdf", "note-mark-added", "note-mark-removed"];
/// Each mark's key, by its name in the earlier app's English and Chinese.
const MARK_NAMES: [(&str, &str, &str); 5] = [("strike", "Strike", "划掉"), ("circle", "Circle", "圈出"), ("underline", "Underline", "下划线"), ("star", "Star", "星标"), ("highlight", "Highlight", "高亮")];

/// A note kept as plain text, in English or Chinese, by an earlier version: which note it is and its values.
fn recognize(text: &str) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    // The earlier app's marks: 加上了「词」的下划线, Added Underline on "word".
    let mark_key = |name: &str| MARK_NAMES.iter().find(|(_, en, zh)| *en == name || *zh == name).map(|(key, ..)| key.to_string());
    for (removed, verb) in [(false, "加上了"), (true, "去掉了")] {
        if let Some(rest) = text.strip_prefix(verb) {
            let (what, mark) = rest.rsplit_once('的')?;
            let what = what.strip_prefix('「').and_then(|w| w.strip_suffix('」')).unwrap_or(what);
            return Some(mark_note(removed, what, &mark_key(mark)?));
        }
    }
    for (removed, verb) in [(false, "Added "), (true, "Removed ")] {
        if let Some(rest) = text.strip_prefix(verb) {
            let (mark, what) = rest.split_once(" on ")?;
            return Some(mark_note(removed, what.trim_matches('"'), &mark_key(mark)?));
        }
    }
    // This app's own notes, as they read in English or Chinese before they were kept as messages.
    for id in NOTES {
        for language in [ENGLISH, 1] {
            if let Some(args) = pattern(language, id).and_then(|p| match_pattern(&p, text)) {
                return Some((id.to_string(), args));
            }
        }
    }
    None
}

fn mark_note(removed: bool, what: &str, mark: &str) -> (String, serde_json::Map<String, serde_json::Value>) {
    let mut args = serde_json::Map::new();
    args.insert("mark".into(), mark.into());
    args.insert("what".into(), what.into());
    (if removed { "note-mark-removed" } else { "note-mark-added" }.to_string(), args)
}

/// A message's text as written in a language's file, if it is a single line.
fn pattern(language: usize, id: &str) -> Option<String> {
    let prefix = format!("{id} = ");
    SOURCES[language].lines().find_map(|l| l.strip_prefix(&prefix)).filter(|p| !p.trim().is_empty() && !p.contains("->")).map(str::to_string)
}

/// The values that make `text` from a pattern such as `Moved “{ $t }” to a new board`, if it does.
fn match_pattern(pattern: &str, text: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    // The pattern as literal pieces and the names between them.
    let mut pieces = vec![];
    let mut names = vec![];
    let mut rest = pattern;
    while let Some(open) = rest.find("{ $") {
        pieces.push(rest[..open].to_string());
        let close = rest[open..].find('}')? + open;
        names.push(rest[open + 3..close].trim().to_string());
        rest = &rest[close + 1..];
    }
    pieces.push(rest.to_string());
    let mut at = text.strip_prefix(pieces[0].as_str())?;
    let mut args = serde_json::Map::new();
    for (name, next) in names.iter().zip(&pieces[1..]) {
        let end = if next.is_empty() { at.len() } else { at.find(next.as_str())? };
        let value = &at[..end];
        if value.is_empty() {
            return None;
        }
        args.insert(name.clone(), value.parse::<f64>().map_or_else(|_| value.into(), |n| serde_json::json!(n)));
        at = &at[end + next.len()..];
    }
    at.is_empty().then_some(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Each message's ID and the variables its text uses, read from a file's text.
    fn messages(source: &str) -> HashMap<String, BTreeSet<String>> {
        let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut current: Option<String> = None;
        for line in source.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            if !line.starts_with(' ') {
                if let Some((id, _)) = line.split_once(" =") {
                    current = Some(id.trim().to_string());
                    out.entry(id.trim().to_string()).or_default();
                }
            }
            if let Some(id) = &current {
                let mut rest = line;
                while let Some(at) = rest.find('$') {
                    let name: String = rest[at + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
                    out.get_mut(id).expect("added above").insert(name);
                    rest = &rest[at + 1..];
                }
            }
        }
        out
    }

    #[test]
    fn every_language_reads_cleanly_and_uses_only_englishs_values() {
        let english = messages(SOURCES[ENGLISH]);
        for ((code, _), source) in LANGUAGES.iter().zip(SOURCES) {
            if let Err((_, errors)) = FluentResource::try_new(source.to_string()) {
                panic!("{code}.ftl does not read: {errors:?}");
            }
            for (id, used) in messages(source) {
                let Some(known) = english.get(&id) else { panic!("{code}.ftl has {id}, which English does not") };
                assert!(used.is_subset(known), "{code}.ftl: {id} uses {used:?}, English gives {known:?}");
            }
        }
    }

    #[test]
    fn every_message_the_code_names_is_in_english() {
        let english = messages(SOURCES[ENGLISH]);
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut missing = vec![];
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            // This file names messages only in its examples.
            if path.ends_with("i18n.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for opening in ["t(\"", "ta!(\""] {
                for (at, _) in source.match_indices(opening) {
                    // Only a whole name: not `.t("`, nor `fn t(`.
                    if source[..at].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
                        continue;
                    }
                    let id: String = source[at + opening.len()..].chars().take_while(|c| *c != '"').collect();
                    if !english.contains_key(&id) {
                        missing.push(format!("{}: {id}", path.file_name().unwrap().to_string_lossy()));
                    }
                }
            }
        }
        // The messages named from tables, by their keys.
        let built = crate::shortcuts::ACTIONS.iter().map(|a| format!("action-{}", a.1)).chain(crate::shortcuts::FIXED.iter().map(|(_, id)| id.to_string()));
        missing.extend(built.filter(|id| !english.contains_key(id)));
        assert!(missing.is_empty(), "not in en.ftl: {missing:?}");
    }

    #[test]
    fn every_message_formats_in_every_language() {
        let english = messages(SOURCES[ENGLISH]);
        for (language, (code, _)) in LANGUAGES.iter().enumerate() {
            let bundle = &bundles()[language];
            for (id, names) in &english {
                let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) else { continue };
                let mut args = FluentArgs::new();
                for name in names {
                    args.set(name.as_str(), 3);
                }
                let mut errors = vec![];
                let text = bundle.format_pattern(pattern, Some(&args), &mut errors);
                assert!(errors.is_empty(), "{code}: {id}: {errors:?}");
                assert!(!text.trim().is_empty(), "{code}: {id} is empty");
            }
        }
    }

    #[test]
    fn counts_take_their_languages_plural() {
        let count = |language: usize, n: usize| {
            let mut args = FluentArgs::new();
            args.set("n", n);
            format(language, "line-count", Some(&args)).unwrap()
        };
        let russian = LANGUAGES.iter().position(|(c, _)| *c == "ru").unwrap();
        assert_eq!(count(ENGLISH, 1), "1 line");
        assert_eq!(count(ENGLISH, 4), "4 lines");
        assert_eq!([count(russian, 1), count(russian, 3), count(russian, 5)], ["1 строка", "3 строки", "5 строк"]);
    }

    #[test]
    fn notes_read_in_the_language_shown_and_are_kept_in_english() {
        let kept = crate::note!("note-moved-to-new-board", t = "Pricing");
        assert_eq!(kept.text, "Moved “Pricing” to a new board");
        let line = serde_json::json!({ "text": kept.text, "manual": true, "note": kept.message });
        // Each language asked for directly: the language shown is shared by every test running at once.
        let chinese = LANGUAGES.iter().position(|(c, _)| *c == "zh-Hans").unwrap();
        assert_eq!(line_text_in(chinese, &line), "把「Pricing」移到了新的一张板");
        // Written as plain text by earlier versions, in either language.
        assert_eq!(line_text_in(chinese, &serde_json::json!({ "text": "Moved “Pricing” to a new board", "manual": true })), "把「Pricing」移到了新的一张板");
        assert_eq!(line_text_in(chinese, &serde_json::json!({ "text": "加上了「One printer for fifteen people?」的下划线", "manual": true })), "加上了「One printer for fifteen people?」的下划线");
        assert_eq!(line_text_in(ENGLISH, &serde_json::json!({ "text": "加上了「One printer for fifteen people?」的下划线", "manual": true })), "Underlined “One printer for fifteen people?”");
        assert_eq!(line_text_in(ENGLISH, &serde_json::json!({ "text": "Removed Highlight on \"a word\"", "manual": true })), "Removed the highlight from “a word”");
        assert_eq!(line_text_in(ENGLISH, &serde_json::json!({ "text": "said as it was", "manual": false })), "said as it was");
        assert_eq!(line_text_in(ENGLISH, &serde_json::json!({ "text": "a note nobody knows", "manual": true })), "a note nobody knows");
    }

    #[test]
    fn every_note_reads_in_every_language() {
        for id in NOTES {
            for (language, (code, _)) in LANGUAGES.iter().enumerate() {
                let mut args = serde_json::Map::new();
                for name in ["to", "from", "when", "t", "name", "read", "pages", "boards", "what"] {
                    args.insert(name.into(), serde_json::json!(2));
                }
                args.insert("mark".into(), "underline".into());
                let text = format(language, id, Some(&fluent_args(&args)));
                assert!(text.is_some_and(|t| !t.is_empty()), "{code}: {id}");
            }
        }
    }

    #[test]
    fn a_system_locale_finds_its_language() {
        assert_eq!(LANGUAGES[best("zh-CN")].0, "zh-Hans");
        assert_eq!(LANGUAGES[best("zh_TW.UTF-8")].0, "zh-Hant");
        assert_eq!(LANGUAGES[best("zh-Hant-HK")].0, "zh-Hant");
        assert_eq!(LANGUAGES[best("ja-JP")].0, "ja");
        assert_eq!(LANGUAGES[best("pt-BR")].0, "pt");
        assert_eq!(LANGUAGES[best("nl-NL")].0, "en", "a language there is no file for is English");
    }

    #[test]
    fn values_are_placed_and_a_missing_message_falls_back() {
        let mut args = FluentArgs::new();
        args.set("n", 2);
        assert_eq!(format(ENGLISH, "no-such-message", Some(&args)), None);
        let japanese = LANGUAGES.iter().position(|(c, _)| *c == "ja").unwrap();
        assert_eq!(message_in(japanese, "no-such-message", None), "no-such-message");
    }
}
