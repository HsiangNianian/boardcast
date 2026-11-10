//! The remote model's settings, in the same `remote.json` the older app uses, so a key saved there
//! keeps working. The file is private to the user and the key is never logged.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// Use the remote model rather than the local one.
    pub enabled: bool,
    /// Tell speakers apart by voice.
    pub diarize: bool,
    /// Recognize speech with the system's recognizer, such as Apple's, rather than the bundled models.
    pub system_speech: bool,
    /// The language the system's recognizer listens for: "auto" for the system's own, or a locale like "ja-JP".
    pub speech_language: String,
    /// The interface's language: "auto" to follow the system, or "en" or "zh".
    pub interface_language: String,
    /// The input to listen to, by its id; empty for the system's default.
    pub input: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { base_url: String::new(), api_key: String::new(), model: String::new(), enabled: false, diarize: true, system_speech: false, speech_language: "auto".into(), interface_language: "auto".into(), input: String::new() }
    }
}

fn config_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("BOARDCAST_CONFIG") {
        return Ok(PathBuf::from(dir));
    }
    #[cfg(target_os = "macos")]
    let base = PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join("Library/Application Support");
    #[cfg(not(target_os = "macos"))]
    let base = match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".config"),
    };
    Ok(base.join("games.mmstudio.boardcast"))
}

impl Settings {
    /// A missing or damaged file means nothing is configured yet.
    pub fn load() -> Self {
        config_dir().ok().and_then(|d| fs::read_to_string(d.join("remote.json")).ok()).map_or_else(Self::default, |text| Self::parse(&text))
    }

    fn parse(text: &str) -> Self {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let text = |key: &str| v[key].as_str().unwrap_or_default().to_string();
        Self {
            base_url: text("baseUrl"),
            api_key: text("apiKey"),
            model: text("model"),
            enabled: v["enabled"].as_bool().unwrap_or(false),
            diarize: v["diarize"].as_bool().unwrap_or(true),
            system_speech: v["systemSpeech"].as_bool().unwrap_or(false),
            speech_language: v["speechLanguage"].as_str().filter(|l| !l.is_empty()).unwrap_or("auto").to_string(),
            interface_language: v["interfaceLanguage"].as_str().filter(|l| !l.is_empty()).unwrap_or("auto").to_string(),
            input: text("input"),
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let text = serde_json::to_string_pretty(&json!({ "baseUrl": self.base_url, "apiKey": self.api_key, "model": self.model, "enabled": self.enabled, "diarize": self.diarize, "systemSpeech": self.system_speech, "speechLanguage": self.speech_language, "interfaceLanguage": self.interface_language, "input": self.input }))?;
        let tmp = dir.join("remote.json.tmp");
        // Private from the first byte: the key must never sit in a file others can read.
        let _ = fs::remove_file(&tmp);
        let mut file = OpenOptions::new().create_new(true).write(true).mode(0o600).open(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, dir.join("remote.json")).context("saving the model settings")?;
        Ok(())
    }

    /// Whether the fields needed for a request are filled in.
    pub fn is_complete(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.api_key.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// The first and last few characters, for showing which key is saved.
    pub fn key_hint(&self) -> Option<String> {
        let chars: Vec<char> = self.api_key.chars().collect();
        match chars.len() {
            0 => None,
            1..=7 => Some("…".to_string()),
            n => Some(format!("{}…{}", chars[..3].iter().collect::<String>(), chars[n - 4..].iter().collect::<String>())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_older_apps_file() {
        let s = Settings::parse(r#"{"baseUrl":"https://api.deepseek.com","apiKey":"sk-test","model":"deepseek-flash","enabled":true}"#);
        assert_eq!(s.base_url, "https://api.deepseek.com");
        assert!(s.enabled && s.is_complete());
        assert_eq!(Settings::parse("not json"), Settings::default());
    }

    #[test]
    fn the_hint_never_shows_the_whole_key() {
        let key = |k: &str| Settings { api_key: k.into(), ..Settings::default() }.key_hint();
        assert_eq!(key(""), None);
        assert_eq!(key("short").as_deref(), Some("…"));
        assert_eq!(key("sk-1234567890abcdef").as_deref(), Some("sk-…cdef"));
    }

    #[test]
    fn saved_settings_round_trip_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("boardcast-settings-{}", std::process::id()));
        std::env::set_var("BOARDCAST_CONFIG", &dir);
        let s = Settings { base_url: "https://x".into(), api_key: "sk-secret".into(), model: "m".into(), enabled: true, diarize: false, system_speech: true, speech_language: "ja-JP".into(), interface_language: "zh".into(), input: "pulse:monitor".into() };
        s.save().unwrap();
        assert_eq!(Settings::load(), s);
        assert_eq!(fs::metadata(dir.join("remote.json")).unwrap().permissions().mode() & 0o777, 0o600);
        std::env::remove_var("BOARDCAST_CONFIG");
        let _ = fs::remove_dir_all(dir);
    }
}
