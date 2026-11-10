//! The look of the window: quiet paper, one accent, and everything else in a few shades of ink. The
//! components come from the kit, and this sets the colors they use.

use gpui_kit::component::{Icon, Theme, ThemeMode};
use gpui_kit::{rgb, App, AssetSource, Hsla, Result, SharedString};
use std::borrow::Cow;

/// What floats over the board, a touch warmer than the paper under it.
pub const SURFACE: u32 = 0xff_fe_fa;
pub const LINE: u32 = 0xe6_e4_dd;
const HOVER: u32 = 0xf1_ef_e8;
pub const SELECTED: u32 = 0xec_ef_e7;
const TEXT: u32 = 0x1e_1f_1b;
pub const MUTED: u32 = 0x62_66_5c;
pub const FAINT: u32 = 0x85_8a_80;
pub const ACCENT: u32 = 0x28_61_43;
pub const STRONG: u32 = 0x24_27_20;
const STRONG_HOVER: u32 = 0x3a_3e_33;
pub const RECORDING: u32 = 0xc7_44_32;
pub const DANGER: u32 = 0xa3_28_26;
pub const DANGER_SOFT: u32 = 0xfb_ec_ea;
pub const WARNING: u32 = 0x80_53_00;
pub const TRACK: u32 = 0xe4_e4_df;
pub const ON_STRONG: u32 = 0xff_ff_ff;

gpui_kit_assets::icon_assets!(ExtraIcons, [FileDown, FilePlus, Import, Pencil, RotateCcw, Sparkles, Trash, Users, WandSparkles, X]);

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

/// Put the paper palette into the kit's theme.
pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Light, None, cx);
    let color = |value: u32| -> Hsla { rgb(value).into() };
    Theme::update(cx, |theme| {
        theme.background = color(SURFACE);
        theme.foreground = color(TEXT);
        theme.border = color(LINE);
        theme.input = color(LINE);
        theme.ring = color(ACCENT);
        theme.muted = color(HOVER);
        theme.muted_foreground = color(MUTED);
        theme.popover = color(SURFACE);
        theme.popover_foreground = color(TEXT);
        theme.accent = color(HOVER);
        theme.accent_foreground = color(TEXT);
        theme.primary = color(STRONG);
        theme.primary_hover = color(STRONG_HOVER);
        theme.primary_active = color(TEXT);
        theme.primary_foreground = color(ON_STRONG);
        theme.danger = color(RECORDING);
        theme.danger_foreground = color(ON_STRONG);
        theme.button_primary = color(STRONG);
        theme.button_primary_hover = color(STRONG_HOVER);
        theme.button_primary_active = color(TEXT);
        theme.button_primary_foreground = color(ON_STRONG);
        theme.button_danger = color(RECORDING);
        theme.button_danger_foreground = color(ON_STRONG);
        theme.button_success = color(ACCENT);
        theme.button_success_foreground = color(ON_STRONG);
        theme.progress_bar = color(ACCENT);
    });
}
