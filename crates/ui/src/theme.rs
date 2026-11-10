//! The look of the window: quiet paper, one accent, and everything else in a few shades of ink. The
//! components come from the kit, and this sets the colors they use.

use gpui_kit::component::{Icon, Theme, ThemeMode};
use boardcast_board::compose::PAPER;
use gpui_kit::{rgb, App, AssetSource, Hsla, Result, SharedString, WindowAppearance};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

/// The colors of the window and the board. Light is paper and marker; dark is a chalkboard.
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
    surface: 0xff_fe_fa,
    line: 0xe6_e4_dd,
    hover: 0xf1_ef_e8,
    selected: 0xec_ef_e7,
    text: 0x1e_1f_1b,
    muted: 0x62_66_5c,
    faint: 0x85_8a_80,
    accent: 0x28_61_43,
    strong: 0x24_27_20,
    strong_hover: 0x3a_3e_33,
    recording: 0xc7_44_32,
    danger: 0xa3_28_26,
    danger_soft: 0xfb_ec_ea,
    warning: 0x80_53_00,
    track: 0xe4_e4_df,
    on_strong: 0xff_ff_ff,
    jumped: 0xff_f3_bf,
    ink: [0x1e_1e_1e, 0xe0_31_31, 0x19_71_c2],
};

const DARK: Palette = Palette {
    paper: 0x24_2b_27,
    surface: 0x2d_31_2e,
    line: 0x40_45_41,
    hover: 0x38_3d_39,
    selected: 0x3a_43_3d,
    text: 0xe9_e9_e4,
    muted: 0xad_b1_a9,
    faint: 0x8d_92_89,
    accent: 0x74_c4_97,
    strong: 0xe9_e9_e4,
    strong_hover: 0xd0_d2_cb,
    recording: 0xe2_5d_4a,
    danger: 0xff_8f_85,
    danger_soft: 0x4b_2c_29,
    warning: 0xe6_b5_5e,
    track: 0x40_45_41,
    on_strong: 0x1e_1f_1b,
    jumped: 0x5c_51_24,
    ink: [0xec_ea_e2, 0xff_8b_7b, 0x93_c7_ff],
};

static DARK_MODE: AtomicBool = AtomicBool::new(false);

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

/// Follow the system's light or dark appearance, and put that palette into the kit's theme.
pub fn apply(cx: &mut App) {
    let dark = matches!(cx.window_appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
    DARK_MODE.store(dark, Ordering::Relaxed);
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
        theme.progress_bar = color(p.accent);
    });
}
