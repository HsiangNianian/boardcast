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
}

impl Quiet {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), icon: None, content: vec![], tooltip: None, on_click: None, disabled: false, selected: false, w: None, h: px(28.), pad: px(8.) }
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
        let (disabled, selected) = (self.disabled, self.selected);
        div()
            .id(self.id.clone())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .h(self.h)
            .map(|d| match self.w {
                Some(w) => d.w(w),
                None => d.px(self.pad),
            })
            .rounded(px(2.))
            .text_color(rgb(if disabled { p.faint } else { p.text }))
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
            .with_spring(ElementId::Name(format!("{}-fill", self.id).into()), SpringAnimation::new(motion::SNAP).to(over && !selected), |d, t| if t.0 > 0.001 { d.bg(motion::hover_fill(t.0)) } else { d })
    }
}
