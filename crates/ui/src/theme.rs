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
    /// The sidebar: the app around the meeting, a shade away from the meeting itself.
    pub chrome: u32,
    pub line: u32,
    pub hover: u32,
    /// One shade past hover, while pressed.
    pub press: u32,
    /// What floats over the board: white on paper, a lighter grey on black.
    pub raised: u32,
    pub raised_line: u32,
    /// The one solid button: ink in the light, a lifted grey in the dark rather than a white slab.
    pub button: u32,
    pub button_hover: u32,
    pub on_button: u32,
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
    chrome: 0xf3_f3_f1,
    line: 0xe2_e2_e2,
    hover: 0xeb_eb_e8,
    press: 0xe0_e0_dc,
    raised: 0xff_ff_ff,
    raised_line: 0xe2_e2_e2,
    button: 0x11_11_11,
    button_hover: 0x33_33_33,
    on_button: 0xff_ff_ff,
    selected: 0xec_ec_ec,
    text: 0x11_11_11,
    muted: 0x55_55_55,
    faint: 0x6e_6e_6e,
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
    chrome: 0x0e_0e_0e,
    line: 0x26_26_26,
    hover: 0x1a_1a_1a,
    press: 0x22_22_22,
    raised: 0x16_16_16,
    raised_line: 0x2a_2a_2a,
    button: 0x2a_2a_2a,
    button_hover: 0x33_33_33,
    on_button: 0xf2_f2_f2,
    selected: 0x1f_1f_1f,
    text: 0xf2_f2_f2,
    muted: 0xc4_c4_c4,
    faint: 0x8f_8f_8f,
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

/// The sizes of type, one scale for the whole window. Mono is for numbers, times, keys and group labels.
pub mod size {
    /// A group's name over its rows, in small capitals.
    pub const LABEL: f32 = 10.5;
    /// Numbers, times and keys.
    pub const META: f32 = 11.;
    /// Hints and secondary lines.
    pub const SMALL: f32 = 12.;
    /// Rows, buttons and tabs.
    pub const UI: f32 = 13.;
    /// What was said and written: transcript, summary.
    pub const BODY: f32 = 14.;
    /// The meeting's name in the title bar.
    pub const TITLE: f32 = 15.;
    /// A section of a page.
    pub const HEADING: f32 = 18.;
    /// A page of its own: Meetings, Settings.
    pub const PAGE: f32 = 26.;
}

/// The steps of space between things, in pixels. Values off these steps in the window are optical nudges
/// (a bullet set on a text's middle, the padding inside a key's label), not spacing.
pub mod space {
    /// A hairline gap, between rows that touch.
    pub const HAIR: f32 = 2.;
    pub const XS: f32 = 4.;
    pub const S: f32 = 8.;
    pub const M: f32 = 12.;
    pub const L: f32 = 16.;
    pub const XL: f32 = 24.;
    pub const XXL: f32 = 32.;
}

/// The type scale on anything styled: `div().text_ui()`.
pub trait Type: gpui_kit::Styled + Sized {
    fn text_label(self) -> Self {
        self.text_size(px(size::LABEL))
    }
    fn text_meta(self) -> Self {
        self.text_size(px(size::META))
    }
    fn text_small(self) -> Self {
        self.text_size(px(size::SMALL))
    }
    fn text_ui(self) -> Self {
        self.text_size(px(size::UI))
    }
    fn text_body(self) -> Self {
        self.text_size(px(size::BODY))
    }
    fn text_title(self) -> Self {
        self.text_size(px(size::TITLE))
    }
    fn text_heading(self) -> Self {
        self.text_size(px(size::HEADING))
    }
    fn text_page(self) -> Self {
        self.text_size(px(size::PAGE))
    }
}

impl<T: gpui_kit::Styled> Type for T {}

/// How far something stands off what is under it.
#[derive(Clone, Copy, PartialEq)]
pub enum Lift {
    /// What floats over the board: tools, the page switcher, the map, menus.
    Float,
    /// A sheet lying on the board, such as an imported page: paper on a desk.
    Sheet,
}

/// The shadow for a lift, `t` of the way in: none at 0, all of it at 1.
pub fn shadow_at(lift: Lift, t: f32) -> Vec<gpui_kit::BoxShadow> {
    shadow(lift)
        .into_iter()
        .map(|mut s| {
            s.color.a *= t.clamp(0., 1.);
            s
        })
        .collect()
}

/// The shadow for a lift. In the dark a shadow hardly shows, so what floats there is also a lighter surface
/// ([`Palette::raised`]); the shadow only deepens the edge.
pub fn shadow(lift: Lift) -> Vec<gpui_kit::BoxShadow> {
    let shade = |alpha: f32, y: f32, blur: f32| gpui_kit::BoxShadow {
        color: gpui_kit::hsla(0., 0., 0., alpha),
        offset: gpui_kit::point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    };
    // A faint light along the top edge of a lifted dark surface, as if it caught the light.
    let rim = gpui_kit::BoxShadow { color: gpui_kit::hsla(0., 0., 1., 0.05), offset: gpui_kit::point(px(0.), px(1.)), blur_radius: px(0.), spread_radius: px(0.), inset: true };
    match (lift, is_dark()) {
        (Lift::Float, false) => vec![shade(0.05, 1., 2.), shade(0.07, 6., 20.)],
        (Lift::Float, true) => vec![rim, shade(0.55, 10., 28.)],
        (Lift::Sheet, false) => vec![shade(0.08, 1., 3.), shade(0.06, 10., 30.)],
        (Lift::Sheet, true) => vec![shade(0.6, 10., 30.)],
    }
}

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

/// The website's pictograms, built from blocks and lines, in place of the toolkit's icons of the same name.
/// Holes are cut with even-odd fills, since an icon is drawn as one shape in the text's color.
const PICTOGRAMS: [(&str, &str); 43] = [
    ("icons/broadcast.svg", include_str!("../icons/broadcast.svg")),
    ("icons/chip.svg", include_str!("../icons/chip.svg")),
    ("icons/info.svg", include_str!("../icons/info.svg")),
    ("icons/keyboard.svg", include_str!("../icons/keyboard.svg")),
    ("icons/mic.svg", include_str!("../icons/mic.svg")),
    ("icons/chevron-down.svg", include_str!("../icons/chevron-down.svg")),
    ("icons/chevron-left.svg", include_str!("../icons/chevron-left.svg")),
    ("icons/chevron-right.svg", include_str!("../icons/chevron-right.svg")),
    ("icons/copy.svg", include_str!("../icons/copy.svg")),
    ("icons/crop.svg", include_str!("../icons/crop.svg")),
    ("icons/eraser.svg", include_str!("../icons/eraser.svg")),
    ("icons/file-down.svg", include_str!("../icons/file-down.svg")),
    ("icons/file-plus.svg", include_str!("../icons/file-plus.svg")),
    ("icons/folder.svg", include_str!("../icons/folder.svg")),
    ("icons/folder-open.svg", include_str!("../icons/folder-open.svg")),
    ("icons/fork.svg", include_str!("../icons/fork.svg")),
    ("icons/highlighter.svg", include_str!("../icons/highlighter.svg")),
    ("icons/import.svg", include_str!("../icons/import.svg")),
    ("icons/lasso.svg", include_str!("../icons/lasso.svg")),
    ("icons/map.svg", include_str!("../icons/map.svg")),
    ("icons/minus.svg", include_str!("../icons/minus.svg")),
    ("icons/panel-left-close.svg", include_str!("../icons/panel-left-close.svg")),
    ("icons/panel-left-open.svg", include_str!("../icons/panel-left-open.svg")),
    ("icons/panel-right-close.svg", include_str!("../icons/panel-right-close.svg")),
    ("icons/panel-right-open.svg", include_str!("../icons/panel-right-open.svg")),
    ("icons/pause.svg", include_str!("../icons/pause.svg")),
    ("icons/pencil.svg", include_str!("../icons/pencil.svg")),
    ("icons/pin.svg", include_str!("../icons/pin.svg")),
    ("icons/play.svg", include_str!("../icons/play.svg")),
    ("icons/plus.svg", include_str!("../icons/plus.svg")),
    ("icons/redo-2.svg", include_str!("../icons/redo-2.svg")),
    ("icons/rotate-ccw.svg", include_str!("../icons/rotate-ccw.svg")),
    ("icons/search.svg", include_str!("../icons/search.svg")),
    ("icons/settings.svg", include_str!("../icons/settings.svg")),
    ("icons/sparkles.svg", include_str!("../icons/sparkles.svg")),
    ("icons/star.svg", include_str!("../icons/star.svg")),
    ("icons/summary.svg", include_str!("../icons/summary.svg")),
    ("icons/transcript.svg", include_str!("../icons/transcript.svg")),
    ("icons/trash.svg", include_str!("../icons/trash.svg")),
    ("icons/undo-2.svg", include_str!("../icons/undo-2.svg")),
    ("icons/users.svg", include_str!("../icons/users.svg")),
    ("icons/wand-sparkles.svg", include_str!("../icons/wand-sparkles.svg")),
    ("icons/x.svg", include_str!("../icons/x.svg")),
];

/// The icons the toolkit carries, the few more this window asks for, and our own pictograms over both.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, svg)) = PICTOGRAMS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(svg.as_bytes())));
        }
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

/// The interface's fonts, registered once at start.
pub fn load_fonts(cx: &mut App) {
    let fonts = vec![Cow::Borrowed(&include_bytes!("../fonts/DMSans.ttf")[..]), Cow::Borrowed(&include_bytes!("../fonts/JetBrainsMono.ttf")[..])];
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("fonts: {e:#}");
    }
}

/// Whether the light or dark choice follows the system.
pub fn follows_system() -> bool {
    FOLLOW_SYSTEM.load(Ordering::Relaxed)
}

/// Settle light or dark, following the system unless Settings chose, and put that palette into the kit's theme.
/// Called at start and again whenever the choice or the system's appearance changes.
pub fn apply(cx: &mut App) {
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
        // Focus in the ink of the text, not a colour of its own.
        theme.ring = color(p.muted);
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
        theme.button_primary = color(p.button);
        theme.button_primary_hover = color(p.button_hover);
        theme.button_primary_active = color(if dark { 0x22_22_22 } else { 0x00_00_00 });
        theme.button_primary_foreground = color(p.on_button);
        theme.button_danger = color(p.recording);
        theme.button_danger_foreground = color(p.on_strong);
        theme.button_success = color(p.accent);
        theme.button_success_foreground = color(p.on_strong);
        theme.progress_bar = color(p.strong);
        theme.font_family = SANS.into();
        theme.font_size = px(size::BODY);
        theme.mono_font_family = MONO.into();
        // Square, as on the website; just enough rounding to keep edges from looking cut.
        theme.radius = px(2.);
        theme.radius_lg = px(3.);
    });
}
