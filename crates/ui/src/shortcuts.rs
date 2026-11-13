//! The keyboard shortcuts: what each does, the keys it starts with, and the keys the user gave it instead.
//! The key handler, the tooltips, the shortcuts window and Settings all read them from here, so a key that is
//! changed is changed everywhere.

use crate::i18n::tr;
use gpui_kit::Keystroke;

/// Something a key does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Listen,
    TypeLine,
    Undo,
    Redo,
    Tidy,
    ZoomIn,
    ZoomOut,
    Fit,
    Map,
    Replay,
    NewMeeting,
    Search,
    Sidebar,
    Settings,
    Shortcuts,
}

/// Where an action is listed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Meeting,
    Board,
    App,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Self::Meeting => tr("Meeting", "会议"),
            Self::Board => tr("Board", "白板"),
            Self::App => tr("App", "应用"),
        }
    }
}

/// Every action: its name in the settings file, what it is called, where it is listed, and its keys at first.
/// `mod` is Ctrl, or ⌘ on a Mac.
pub const ACTIONS: [(Action, &str, &str, &str, Group, &str); 15] = [
    (Action::Listen, "listen", "Start or stop listening", "开始或停止收音", Group::Meeting, "space"),
    (Action::TypeLine, "type-line", "Type a line", "输入一句话", Group::Meeting, "/"),
    (Action::Replay, "replay", "Watch the board grow again", "看板书重新长一遍", Group::Meeting, "mod-shift-r"),
    (Action::NewMeeting, "new-meeting", "New meeting", "新建会议", Group::Meeting, "mod-n"),
    (Action::Undo, "undo", "Undo", "撤销", Group::Board, "mod-z"),
    (Action::Redo, "redo", "Redo", "重做", Group::Board, "mod-shift-z"),
    (Action::Tidy, "tidy", "Tidy the layout", "整理排版", Group::Board, ""),
    (Action::ZoomIn, "zoom-in", "Zoom in", "放大", Group::Board, "mod-="),
    (Action::ZoomOut, "zoom-out", "Zoom out", "缩小", Group::Board, "mod--"),
    (Action::Fit, "fit", "Fit the board", "适应屏幕", Group::Board, "mod-0"),
    (Action::Map, "map", "Map of the board", "板书小地图", Group::Board, "m"),
    (Action::Search, "search", "Search meetings", "搜索会议", Group::App, "mod-k"),
    (Action::Sidebar, "sidebar", "Meetings and settings", "会议与设置", Group::App, "mod-b"),
    (Action::Settings, "settings", "Settings", "设置", Group::App, "mod-,"),
    (Action::Shortcuts, "shortcuts", "Keyboard shortcuts", "键盘快捷键", Group::App, "shift-/"),
];

/// Keys that also work while an action keeps its first keys, as before they could be changed.
const ALSO: [(Action, &str); 2] = [(Action::Redo, "mod-y"), (Action::ZoomIn, "mod-+")];

/// Keys that cannot be given to an action: they close things, and play a replay.
pub const FIXED: [(&str, &str, &str); 4] = [
    ("escape", "Close, or let go of what is selected", "关闭，或取消选中"),
    ("space", "Play or pause a replay", "回放时播放或暂停"),
    ("left", "Step back in a replay", "回放时后退一步"),
    ("right", "Step forward in a replay", "回放时前进一步"),
];

impl Action {
    pub fn id(self) -> &'static str {
        ACTIONS.iter().find(|a| a.0 == self).map_or("", |a| a.1)
    }

    pub fn name(self) -> &'static str {
        ACTIONS.iter().find(|a| a.0 == self).map_or("", |a| tr(a.2, a.3))
    }

    fn first_keys(self) -> &'static str {
        ACTIONS.iter().find(|a| a.0 == self).map_or("", |a| a.5)
    }
}

/// A key with the modifiers held for it, written as the settings file keeps it: "mod-shift-z", "m", "shift-/".
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Keys {
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: String,
}

impl Keys {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_lowercase();
        if text.is_empty() {
            return None;
        }
        // The key is the last part; "-" and "+" can be keys themselves.
        let (mods, key) = if let Some(m) = text.strip_suffix("--") { (format!("{m}-"), "-") } else { match text.rsplit_once('-') { Some((m, k)) if !k.is_empty() => (format!("{m}-"), k), _ => (String::new(), text.as_str()) } };
        let key = if key.is_empty() { "-" } else { key };
        Some(Self { command: mods.contains("mod-"), shift: mods.contains("shift-"), alt: mods.contains("alt-"), key: key.to_string() })
    }

    /// The keys of a key press. Ctrl and ⌘ are one, as before; Shift+/ arrives as "?" on some keyboards.
    pub fn of(stroke: &Keystroke) -> Self {
        let m = &stroke.modifiers;
        let mut keys = Self { command: m.control || m.platform, shift: m.shift, alt: m.alt, key: stroke.key.to_lowercase() };
        if keys.key == "?" {
            keys.key = "/".into();
            keys.shift = true;
        }
        keys
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        if self.command {
            out.push_str("mod-");
        }
        if self.alt {
            out.push_str("alt-");
        }
        if self.shift {
            out.push_str("shift-");
        }
        out.push_str(&self.key);
        out
    }

    /// As the keyboard shows it: ⇧⌘R on a Mac, Ctrl+Shift+R elsewhere.
    pub fn label(&self) -> String {
        let key = match self.key.as_str() {
            "space" => tr("Space", "空格").to_string(),
            "escape" => "Esc".to_string(),
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            "enter" => "Enter".to_string(),
            "tab" => "Tab".to_string(),
            "backspace" => "⌫".to_string(),
            "/" if self.shift => return if cfg!(target_os = "macos") { "?".into() } else { "Shift+/".into() },
            k if k.chars().count() == 1 => k.to_uppercase(),
            k => {
                let mut c = k.chars();
                c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
            }
        };
        if cfg!(target_os = "macos") {
            format!("{}{}{}{key}", if self.alt { "⌥" } else { "" }, if self.shift { "⇧" } else { "" }, if self.command { "⌘" } else { "" })
        } else {
            let mut parts: Vec<String> = vec![];
            if self.command {
                parts.push("Ctrl".into());
            }
            if self.alt {
                parts.push("Alt".into());
            }
            if self.shift {
                parts.push("Shift".into());
            }
            parts.push(key);
            parts.join("+")
        }
    }

    /// A key on its own that a text field or the board needs whatever the shortcuts are.
    pub fn reserved(&self) -> bool {
        !self.command && !self.alt && matches!(self.key.as_str(), "escape" | "enter" | "tab" | "backspace" | "delete" | "left" | "right" | "up" | "down")
    }

    /// Only modifier keys, pressed while a shortcut is being recorded, before the key itself.
    pub fn modifier_only(&self) -> bool {
        matches!(self.key.as_str(), "shift" | "control" | "alt" | "platform" | "function" | "cmd" | "ctrl" | "super")
    }
}

/// The keys every action has now: the user's where they gave some, the first keys otherwise.
#[derive(Clone, Default)]
pub struct Shortcuts {
    /// The user's keys, by action id; an empty text takes a shortcut away.
    custom: Vec<(String, String)>,
}

impl Shortcuts {
    pub fn new(custom: &[(String, String)]) -> Self {
        Self { custom: custom.to_vec() }
    }

    pub fn custom(&self) -> &[(String, String)] {
        &self.custom
    }

    pub fn keys(&self, action: Action) -> Option<Keys> {
        match self.custom.iter().find(|(id, _)| id == action.id()) {
            Some((_, text)) => Keys::parse(text),
            None => Keys::parse(action.first_keys()),
        }
    }

    pub fn changed(&self, action: Action) -> bool {
        self.custom.iter().any(|(id, _)| id == action.id())
    }

    /// The action these keys start, if any.
    pub fn action(&self, pressed: &Keys) -> Option<Action> {
        ACTIONS.iter().map(|a| a.0).find(|&a| self.keys(a).as_ref() == Some(pressed)).or_else(|| {
            ALSO.iter().find(|(a, text)| !self.changed(*a) && Keys::parse(text).as_ref() == Some(pressed)).map(|(a, _)| *a)
        })
    }

    /// An action's keys as the keyboard shows them, or nothing when it has none.
    pub fn label(&self, action: Action) -> Option<String> {
        self.keys(action).map(|k| k.label())
    }

    /// A name with its keys, for a tooltip: "Undo (Ctrl+Z)".
    pub fn tip(&self, action: Action, name: &str) -> String {
        match self.label(action) {
            Some(keys) => format!("{name} ({keys})"),
            None => name.to_string(),
        }
    }

    /// Give an action new keys. Another action's keys, or a key the board keeps for itself, are refused with
    /// the reason.
    pub fn set(&mut self, action: Action, keys: Keys) -> Result<(), String> {
        if keys.reserved() {
            return Err(format!("{} {}", keys.label(), tr("is kept for closing things and moving around.", "用于关闭和移动，不能设为快捷键。")));
        }
        if let Some(other) = self.action(&keys).filter(|&a| a != action) {
            return Err(format!("{} {} {}", keys.label(), tr("is already", "已经用于"), other.name()));
        }
        self.custom.retain(|(id, _)| id != action.id());
        if Keys::parse(action.first_keys()).as_ref() != Some(&keys) {
            self.custom.push((action.id().to_string(), keys.text()));
        }
        Ok(())
    }

    /// Back to the keys an action started with.
    pub fn reset(&mut self, action: Action) {
        self.custom.retain(|(id, _)| id != action.id());
    }

    pub fn reset_all(&mut self) {
        self.custom.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> Keys {
        Keys::parse(text).unwrap()
    }

    #[test]
    fn keys_read_and_write_as_the_settings_file_keeps_them() {
        for text in ["mod-shift-z", "m", "shift-/", "mod--", "mod-=", "mod-,", "space", "alt-mod-k"] {
            let k = keys(text);
            assert_eq!(Keys::parse(&k.text()).unwrap(), k, "{text}");
        }
        assert_eq!(keys("mod--").key, "-");
        assert!(keys("mod-shift-r").command && keys("mod-shift-r").shift);
    }

    #[test]
    fn every_first_key_reaches_its_action_and_no_two_share_keys() {
        let s = Shortcuts::default();
        for (action, _, _, _, _, first) in ACTIONS {
            if let Some(k) = Keys::parse(first) {
                assert_eq!(s.action(&k), Some(action), "{first}");
            }
        }
        assert_eq!(s.action(&keys("mod-y")), Some(Action::Redo), "Ctrl+Y still redoes");
    }

    #[test]
    fn new_keys_replace_the_first_ones_and_clashes_are_refused() {
        let mut s = Shortcuts::default();
        s.set(Action::Map, keys("mod-shift-m")).unwrap();
        assert_eq!(s.action(&keys("mod-shift-m")), Some(Action::Map));
        assert_eq!(s.action(&keys("m")), None, "the old key is free again");
        assert!(s.set(Action::Search, keys("mod-z")).unwrap_err().contains(Action::Undo.name()));
        assert!(s.set(Action::Map, keys("escape")).is_err());
        s.set(Action::Redo, keys("mod-alt-z")).unwrap();
        assert_eq!(s.action(&keys("mod-y")), None, "a changed action loses its extra keys");
        s.reset(Action::Map);
        assert_eq!(s.action(&keys("m")), Some(Action::Map));
        // Setting the first keys again leaves nothing custom behind.
        s.set(Action::Redo, keys("mod-shift-z")).unwrap();
        assert!(!s.changed(Action::Redo));
    }
}
