//! Interface text. English is the base; Chinese follows the system language until the full
//! translation tables move over from the older app.

use std::sync::OnceLock;

fn chinese() -> bool {
    static CHINESE: OnceLock<bool> = OnceLock::new();
    *CHINESE.get_or_init(|| {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            .is_some_and(|v| v.starts_with("zh"))
    })
}

/// The text for the current language.
pub fn tr(en: &'static str, zh: &'static str) -> &'static str {
    if chinese() { zh } else { en }
}
