//! A quiet button: an icon, a word or both, with no fill until the pointer is over it, when the fill fades in
//! rather than snapping, and one shade darker while pressed. The kit's ghost buttons change colour in one
//! cut, so the window's own small commands are drawn with this instead. It keeps whether it is hovered in
//! its own element state, so the view around it needs no bookkeeping.

use crate::motion;
use crate::theme::{icon, pal, Type as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, px, rgb, AnimationExt as _, AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    SpringAnimation, StatefulInteractiveElement as _, Styled as _, Window,
};

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A colour a little darker, for a red button under the pointer.
fn darker(c: u32) -> u32 {
    let ch = |s: u32| ((((c >> s) & 0xff) as f32 * 0.88).round() as u32) << s;
    ch(16) | ch(8) | ch(0)
}

#[derive(IntoElement)]
pub struct Quiet {
    id: ElementId,
    icon: Option<(&'static str, f32)>,
    content: Vec<AnyElement>,
    tooltip: Option<SharedString>,
    on_click: Option<Click>,
    disabled: bool,
    selected: bool,
    w: Option<Pixels>,
    h: Pixels,
    pad: Pixels,
    look: Look,
    min_w: Option<Pixels>,
    /// Square corners on the right, for the left half of a split button.
    square_right: bool,
}

/// How a button stands: quiet with no fill, in a hairline, solid, red for what cannot be undone, or one of a
/// few choices side by side, raised when chosen.
#[derive(Clone, Copy, PartialEq)]
enum Look {
    Quiet,
    Outline,
    Solid,
    Danger,
    Choice(bool),
    /// Filled with a colour of the caller's, which may be moving, and ink to go on it.
    Fill(u32, u32),
}

impl Quiet {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), icon: None, content: vec![], tooltip: None, on_click: None, disabled: false, selected: false, w: None, h: px(28.), pad: px(8.), look: Look::Quiet, min_w: None, square_right: false }
    }

    /// An icon by its file name, drawn at `size`.
    pub fn icon(mut self, name: &'static str, size: f32) -> Self {
        self.icon = Some((name, size));
        self
    }

    /// A word beside the icon.
    pub fn label(mut self, text: impl Into<SharedString>) -> Self {
        let text: SharedString = text.into();
        self.content.push(div().text_ui().whitespace_nowrap().child(text).into_any_element());
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.content.push(child.into_any_element());
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// A square of `side`.
    pub fn square(mut self, side: Pixels) -> Self {
        self.w = Some(side);
        self.h = side;
        self
    }

    pub fn h(mut self, h: Pixels) -> Self {
        self.h = h;
        self
    }

    /// With a hairline round it, for a command that stands on its own in a page or a card.
    pub fn outline(mut self) -> Self {
        self.look = Look::Outline;
        self.pad = px(10.);
        self
    }

    /// Filled: the main thing to do in a page or a dialog.
    pub fn solid(mut self) -> Self {
        self.look = Look::Solid;
        self.pad = px(10.);
        self
    }

    /// Filled in red: for what cannot be undone.
    pub fn danger(mut self) -> Self {
        self.look = Look::Danger;
        self.pad = px(10.);
        self
    }

    /// One of a few choices side by side; the chosen one is raised like the sidebar's selection.
    pub fn choice(mut self, chosen: bool) -> Self {
        self.look = Look::Choice(chosen);
        self.h = px(24.);
        self
    }

    /// Filled with `fill`, ink in `ink`; under the pointer it deepens a little.
    pub fn fill(mut self, fill: u32, ink: u32) -> Self {
        self.look = Look::Fill(fill, ink);
        self
    }

    /// Square on the right, for the left half of a split button.
    pub fn square_right(mut self) -> Self {
        self.square_right = true;
        self
    }

    /// At least this wide, so a label that changes does not make it jump.
    pub fn min_w(mut self, w: Pixels) -> Self {
        self.min_w = Some(w);
        self
    }

    /// Room on either side of what it shows.
    pub fn pad(mut self, pad: Pixels) -> Self {
        self.pad = pad;
        self
    }
}

impl RenderOnce for Quiet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let hovered = window.use_keyed_state(ElementId::Name(format!("{}-hover", self.id).into()), cx, |_, _| false);
        let over = *hovered.read(cx) && !self.disabled;
        let p = pal();
        let (disabled, selected, look) = (self.disabled, self.selected, self.look);
        // What it is filled with at rest and under the pointer, and the ink on it.
        let (rest, under, ink): (Option<u32>, u32, u32) = match look {
            Look::Solid => (Some(p.button), p.button_hover, p.on_button),
            Look::Danger => (Some(p.recording), darker(p.recording), p.on_strong),
            Look::Outline => (Some(p.raised), p.hover, p.text),
            Look::Choice(true) => (Some(p.raised), p.raised, p.text),
            Look::Choice(false) => (None, p.hover, p.muted),
            Look::Quiet => (None, p.hover, p.text),
            Look::Fill(fill, ink) => (Some(fill), darker(fill), ink),
        };
        // Held in a row of its own so it keeps the width of what it shows, as the kit's buttons do, even where
        // what it is put in would stretch it.
        let button = div()
            .id(self.id.clone())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(crate::theme::space::S))
            .h(self.h)
            .map(|d| match self.w {
                Some(w) => d.w(w),
                None => d.px(self.pad),
            })
            .rounded(px(2.))
            .when(self.square_right, |d| d.rounded_tr(px(0.)).rounded_br(px(0.)))
            .when_some(self.min_w, |d, w| d.min_w(w))
            .when(look == Look::Outline, |d| d.border_1().border_color(rgb(p.line)))
            .when(look == Look::Choice(true), |d| d.border_1().border_color(rgb(p.raised_line)).font_weight(gpui_kit::FontWeight::MEDIUM))
            .when(matches!(look, Look::Solid | Look::Danger | Look::Outline | Look::Fill(..)), |d| d.text_ui())
            .when_some(rest, |d, fill| d.bg(rgb(fill)))
            .text_color(rgb(if disabled && matches!(look, Look::Quiet | Look::Outline | Look::Choice(_)) { p.faint } else { ink }))
            .when(selected, |d| d.bg(rgb(p.selected)))
            .when(disabled, |d| d.opacity(0.55))
            .when(!disabled, |d| d.cursor_pointer().active(|d| d.bg(rgb(p.press))))
            .on_hover(move |over: &bool, _, cx| {
                let over = *over;
                hovered.update(cx, |state, cx| {
                    if *state != over {
                        *state = over;
                        cx.notify();
                    }
                });
            })
            .when_some(self.tooltip, |d, tip| d.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)))
            .when_some(self.on_click.filter(|_| !disabled), |d, f| d.on_click(f))
            .when_some(self.icon, |d, (name, size)| d.child(icon(name).size(px(size))))
            .children(self.content)
            .with_spring(ElementId::Name(format!("{}-fill", self.id).into()), SpringAnimation::new(motion::SNAP).to(over && !selected), move |d, t| {
                if t.0 <= 0.001 {
                    return d;
                }
                match rest {
                    // A filled button deepens towards its hover shade; an empty one fades the shade in.
                    Some(fill) => d.bg(motion::mix(fill, under, t.0)),
                    None => {
                        let mut shade: gpui_kit::Hsla = rgb(under).into();
                        shade.a = t.0.clamp(0., 1.);
                        d.bg(shade)
                    }
                }
            });
        div().flex().flex_none().child(button)
    }
}
