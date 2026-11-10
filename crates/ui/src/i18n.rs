//! Interface text. English is the base; Chinese follows the system language, or the choice in Settings,
//! until the full translation tables move over from the older app.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

const SYSTEM: u8 = 0;
const ENGLISH: u8 = 1;
const CHINESE: u8 = 2;

static CHOICE: AtomicU8 = AtomicU8::new(SYSTEM);

/// The system's preferred language. A Mac app opened from the Finder has no `LANG`, so this asks the
/// platform rather than the environment.
fn system_chinese() -> bool {
    static CHINESE_SYSTEM: OnceLock<bool> = OnceLock::new();
    *CHINESE_SYSTEM.get_or_init(|| {
        sys_locale::get_locale().is_some_and(|l| l.to_ascii_lowercase().starts_with("zh"))
    })
}

fn chinese() -> bool {
    match CHOICE.load(Ordering::Relaxed) {
        ENGLISH => false,
        CHINESE => true,
        _ => system_chinese(),
    }
}

/// Use "en" or "zh", or follow the system for anything else. Text drawn from then on follows it.
pub fn choose(language: &str) {
    let choice = match language {
        "en" => ENGLISH,
        "zh" => CHINESE,
        _ => SYSTEM,
    };
    CHOICE.store(choice, Ordering::Relaxed);
}

/// The text for the current language.
pub fn tr(en: &'static str, zh: &'static str) -> &'static str {
    if chinese() {
        zh
    } else {
        en
    }
}
