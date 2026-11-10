//! The look of the window, shared with the website: black and white, light type, one red for what is
//! marked or recording, and square corners. The components come from the kit; this sets what they use.

use gpui_kit::component::{Icon, Theme, ThemeMode};
use boardcast_board::compose::PAPER;
use gpui_kit::{px, rgb, App, AssetSource, Hsla, Result, SharedString, WindowAppearance};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

/// The colors of the window and the board. Dark is the website's black sheet with chalk on it; light is
/// the same sheet in white paper and marker.
pub struct Palette {
    /// The board itself.
    pub paper: u32,
    /// What floats over the board, a touch apart from the paper under it.
    pub surface: u32,
    pub line: u32,
    hover: u32,
    pub selected: u32,
    pub text: u32,
    pub muted: u32,
    pub faint: u32,
    pub accent: u32,
    pub strong: u32,
    strong_hover: u32,
    pub recording: u32,
    pub danger: u32,
    pub danger_soft: u32,
    pub warning: u32,
    pub track: u32,
    pub on_strong: u32,
    /// A transcript line just jumped to.
    pub jumped: u32,
    /// Marker ink and its chalk: black, red and blue.
    ink: [u32; 3],
}

const LIGHT: Palette = Palette {
    paper: PAPER,
    surface: 0xff_ff_ff,
    line: 0xe2_e2_e2,
    hover: 0xf2_f2_f2,
    selected: 0xec_ec_ec,
    text: 0x11_11_11,
    muted: 0x55_55_55,
    faint: 0x8a_8a_8a,
    accent: 0x3b_5b_db,
    strong: 0x11_11_11,
    strong_hover: 0x33_33_33,
    recording: 0xe8_43_2a,
    danger: 0xc8_32_1e,
    danger_soft: 0xfd_ec_eb,
    warning: 0x8a_5a_00,
    track: 0xe2_e2_e2,
    on_strong: 0xff_ff_ff,
    jumped: 0xff_f3_bf,
    ink: [0x1e_1e_1e, 0xe0_31_31, 0x19_71_c2],
};

const DARK: Palette = Palette {
    paper: 0x07_07_07,
    surface: 0x00_00_00,
    line: 0x26_26_26,
    hover: 0x14_14_14,
    selected: 0x1f_1f_1f,
    text: 0xf2_f2_f2,
    muted: 0xc4_c4_c4,
    faint: 0x8a_8a_8a,
    accent: 0x9f_b4_ff,
    strong: 0xf2_f2_f2,
    strong_hover: 0xc4_c4_c4,
    recording: 0xff_5a_36,
    danger: 0xff_5a_36,
    danger_soft: 0x2a_12_0d,
    warning: 0xe6_b5_5e,
    track: 0x26_26_26,
    on_strong: 0x00_00_00,
    jumped: 0x2a_24_10,
    ink: [0xec_ec_e6, 0xff_5a_36, 0x9f_b4_ff],
};

/// The interface face and the one for labels, numbers and keys, bundled so every platform looks alike.
pub const SANS: &str = "DM Sans";
pub const MONO: &str = "JetBrains Mono";

static DARK_MODE: AtomicBool = AtomicBool::new(false);
/// Whether the system's appearance decides, rather than a choice in Settings.
static FOLLOW_SYSTEM: AtomicBool = AtomicBool::new(true);

/// Take the choice from Settings: "light" or "dark" settle the palette now, before anything is drawn;
/// anything else waits for the system's appearance in `apply`.
pub fn choose(appearance: &str) {
    FOLLOW_SYSTEM.store(!matches!(appearance, "light" | "dark"), Ordering::Relaxed);
    DARK_MODE.store(appearance == "dark", Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

/// The palette in use, chosen once the system's appearance is known.
pub fn pal() -> &'static Palette {
    if DARK_MODE.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

impl Palette {
    /// The color a board stroke takes on screen. Exports keep the marker colors whatever the window shows.
    pub fn ink(&self, rgb: u32) -> u32 {
        match LIGHT.ink.iter().position(|&c| c == rgb) {
            Some(i) => self.ink[i],
            None => rgb,
        }
    }
}

gpui_kit_assets::icon_assets!(ExtraIcons, [FileDown, FilePlus, Import, Lasso, Pencil, RotateCcw, Sparkles, Trash, Users, WandSparkles, X]);

/// The icons the toolkit carries, and the few more this window asks for.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

/// An icon by its file name.
pub fn icon(name: &str) -> Icon {
    Icon::empty().path(format!("icons/{name}.svg"))
}

/// Settle light or dark, following the system unless Settings chose, and put that palette into the kit's theme.
pub fn apply(cx: &mut App) {
    let fonts = vec![Cow::Borrowed(&include_bytes!("../fonts/DMSans.ttf")[..]), Cow::Borrowed(&include_bytes!("../fonts/JetBrainsMono.ttf")[..])];
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("fonts: {e:#}");
    }
    if FOLLOW_SYSTEM.load(Ordering::Relaxed) {
        DARK_MODE.store(matches!(cx.window_appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark), Ordering::Relaxed);
    }
    let dark = is_dark();
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    let p = pal();
    let color = |value: u32| -> Hsla { rgb(value).into() };
    Theme::update(cx, |theme| {
        theme.background = color(p.surface);
        theme.foreground = color(p.text);
        theme.border = color(p.line);
        theme.input = color(p.line);
        theme.ring = color(p.accent);
        theme.muted = color(p.hover);
        theme.muted_foreground = color(p.muted);
        theme.popover = color(p.surface);
        theme.popover_foreground = color(p.text);
        theme.accent = color(p.hover);
        theme.accent_foreground = color(p.text);
        theme.primary = color(p.strong);
        theme.primary_hover = color(p.strong_hover);
        theme.primary_active = color(p.text);
        theme.primary_foreground = color(p.on_strong);
        theme.danger = color(p.recording);
        theme.danger_foreground = color(p.on_strong);
        theme.button_primary = color(p.strong);
        theme.button_primary_hover = color(p.strong_hover);
        theme.button_primary_active = color(p.text);
        theme.button_primary_foreground = color(p.on_strong);
        theme.button_danger = color(p.recording);
        theme.button_danger_foreground = color(p.on_strong);
        theme.button_success = color(p.accent);
        theme.button_success_foreground = color(p.on_strong);
        theme.progress_bar = color(p.strong);
        theme.font_family = SANS.into();
        theme.mono_font_family = MONO.into();
        // Square, as on the website; just enough rounding to keep edges from looking cut.
        theme.radius = px(2.);
        theme.radius_lg = px(3.);
    });
}
