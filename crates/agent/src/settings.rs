//! The remote model's settings, in the same `remote.json` the older app uses, so a key saved there
//! keeps working. The file is private to the user and the key is never logged.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
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
    /// Turn to the page being talked about by itself, rather than offering it.
    pub turn_pages: bool,
    /// How names are written: see [`crate::sound::Glossary::parse`].
    pub glossary: String,
    /// Meetings to start from, saved from a board's topics: a name, and the topics.
    pub templates: Vec<(String, Vec<String>)>,
    /// Recognize speech with the system's recognizer, such as Apple's, rather than the bundled models.
    pub system_speech: bool,
    /// The language the system's recognizer listens for: "auto" for the system's own, or a locale like "ja-JP".
    pub speech_language: String,
    /// The interface's language: "auto" to follow the system, or "en" or "zh".
    pub interface_language: String,
    /// The window's look: "auto" to follow the system, or "light" or "dark".
    pub appearance: String,
    /// The input to listen to, by its id; empty for the system's default.
    pub input: String,
    /// Search past meetings by what they mean, with a model that comes with the app. On until turned off.
    pub meeting_search: bool,
    /// How messy the board's handwriting is, from 0 (neat) to 1.
    pub messy: f32,
    /// The window's sidebar is put away. It is out until hidden.
    pub sidebar_hidden: bool,
    /// The map of the board is put away. It is out until closed.
    pub minimap_hidden: bool,
    /// Where the map was dragged to: whether it keeps to the board's right edge and to its bottom edge, and
    /// how far it is from those edges. None for where it starts, at the top right.
    pub minimap_at: Option<(bool, bool, f32, f32)>,
    /// The sidebar's groups of meetings folded away, by name: pinned, today, yesterday, earlier.
    pub folded_groups: Vec<String>,
    /// Keyboard shortcuts the user changed, as action and keys: ("map", "mod-shift-m"). The rest keep theirs.
    pub shortcuts: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { base_url: String::new(), api_key: String::new(), model: String::new(), enabled: false, diarize: true, turn_pages: false, glossary: String::new(), templates: vec![], system_speech: false, speech_language: "auto".into(), interface_language: "auto".into(), appearance: "auto".into(), input: String::new(), meeting_search: true, messy: 0.0, sidebar_hidden: false, minimap_hidden: false, minimap_at: None, folded_groups: vec![], shortcuts: vec![] }
    }
}

fn config_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("BOARDCAST_CONFIG") {
        return Ok(PathBuf::from(dir));
    }
    #[cfg(target_os = "macos")]
    let base = PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join("Library/Application Support");
    #[cfg(windows)]
    let base = PathBuf::from(std::env::var("APPDATA").context("APPDATA is not set")?);
    #[cfg(not(any(target_os = "macos", windows)))]
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
            turn_pages: v["turnPages"].as_bool().unwrap_or(false),
            glossary: text("glossary"),
            templates: v["templates"]
                .as_array()
                .map(|all| {
                    all.iter()
                        .filter_map(|t| Some((t["name"].as_str()?.to_string(), t["topics"].as_array()?.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())))
                        .collect()
                })
                .unwrap_or_default(),
            system_speech: v["systemSpeech"].as_bool().unwrap_or(false),
            speech_language: v["speechLanguage"].as_str().filter(|l| !l.is_empty()).unwrap_or("auto").to_string(),
            interface_language: v["interfaceLanguage"].as_str().filter(|l| !l.is_empty()).unwrap_or("auto").to_string(),
            appearance: v["appearance"].as_str().filter(|l| !l.is_empty()).unwrap_or("auto").to_string(),
            input: text("input"),
            // 0.28 kept it as `meetingSearch`, off by default and saved either way, so a false there was rarely
            // anyone's choice. It is not read: search is on, as it now is for everyone, until turned off here.
            meeting_search: v["searchMeetings"].as_bool().unwrap_or(true),
            messy: v["messy"].as_f64().map_or(0.0, |m| m.clamp(0.0, 1.0) as f32),
            sidebar_hidden: v["sidebarHidden"].as_bool().unwrap_or(false),
            minimap_hidden: v["minimapHidden"].as_bool().unwrap_or(false),
            minimap_at: (|| {
                let at = &v["minimapAt"];
                let far = |key: &str| at[key].as_f64().filter(|d| d.is_finite() && *d >= 0.0).map(|d| d as f32);
                Some((at["right"].as_bool()?, at["bottom"].as_bool()?, far("x")?, far("y")?))
            })(),
            shortcuts: v["shortcuts"].as_object().map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default(),
            folded_groups: v["foldedGroups"].as_array().map(|a| a.iter().filter_map(|g| g.as_str().map(str::to_string)).collect()).unwrap_or_default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let text = serde_json::to_string_pretty(&json!({ "baseUrl": self.base_url, "apiKey": self.api_key, "model": self.model, "enabled": self.enabled, "diarize": self.diarize, "turnPages": self.turn_pages, "glossary": self.glossary, "templates": self.templates.iter().map(|(name, topics)| json!({ "name": name, "topics": topics })).collect::<Vec<_>>(), "systemSpeech": self.system_speech, "speechLanguage": self.speech_language, "interfaceLanguage": self.interface_language, "appearance": self.appearance, "input": self.input, "searchMeetings": self.meeting_search, "messy": self.messy, "sidebarHidden": self.sidebar_hidden, "minimapHidden": self.minimap_hidden, "minimapAt": self.minimap_at.map(|(right, bottom, x, y)| json!({ "right": right, "bottom": bottom, "x": x, "y": y })), "foldedGroups": self.folded_groups, "shortcuts": self.shortcuts.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>() }))?;
        let tmp = dir.join("remote.json.tmp");
        // Private from the first byte: the key must never sit in a file others can read.
        let _ = fs::remove_file(&tmp);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        // Only this user may read the key. A Windows profile folder is private to its user already.
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
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
    fn meeting_search_is_on_unless_turned_off() {
        assert!(Settings::default().meeting_search);
        assert!(Settings::parse("{}").meeting_search);
        // What 0.28 saved for everyone, whether or not they chose it.
        assert!(Settings::parse(r#"{"meetingSearch":false}"#).meeting_search);
        assert!(!Settings::parse(r#"{"searchMeetings":false}"#).meeting_search);
    }

    #[test]
    fn the_hint_never_shows_the_whole_key() {
        let key = |k: &str| Settings { api_key: k.into(), ..Settings::default() }.key_hint();
        assert_eq!(key(""), None);
        assert_eq!(key("short").as_deref(), Some("…"));
        assert_eq!(key("sk-1234567890abcdef").as_deref(), Some("sk-…cdef"));
    }

    #[cfg(unix)]
    #[test]
    fn saved_settings_round_trip_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("boardcast-settings-{}", std::process::id()));
        std::env::set_var("BOARDCAST_CONFIG", &dir);
        let s = Settings { base_url: "https://x".into(), api_key: "sk-secret".into(), model: "m".into(), enabled: true, diarize: false, turn_pages: true, glossary: "鉴权; API = A P I".into(), templates: vec![("Sprint review".into(), vec!["Done".into(), "Next".into()])], system_speech: true, speech_language: "ja-JP".into(), interface_language: "zh".into(), appearance: "dark".into(), input: "pulse:monitor".into(), meeting_search: false, messy: 0.25, sidebar_hidden: true, minimap_hidden: true, minimap_at: Some((false, true, 24.0, 80.5)), folded_groups: vec!["earlier".into(), "pinned".into()], shortcuts: vec![("map".into(), "mod-shift-m".into()), ("tidy".into(), "mod-t".into())] };
        s.save().unwrap();
        assert_eq!(Settings::load(), s);
        assert_eq!(fs::metadata(dir.join("remote.json")).unwrap().permissions().mode() & 0o777, 0o600);
        std::env::remove_var("BOARDCAST_CONFIG");
        let _ = fs::remove_dir_all(dir);
    }
}
