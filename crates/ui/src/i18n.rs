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
    let language = current();
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
        choose("ja");
        assert_eq!(message("no-such-message", None), "no-such-message");
        choose("en");
    }
}
