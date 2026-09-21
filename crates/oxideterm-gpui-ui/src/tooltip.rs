use gpui::{
    AnyElement, AnyView, App, AppContext, Context, Div, ParentElement, Render, Styled, Window, div,
    prelude::*, px, rgb,
};
use oxideterm_theme::ThemeTokens;

struct TooltipView {
    tokens: ThemeTokens,
    label: String,
    shortcut: Option<String>,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl gpui::IntoElement {
        tooltip_content(&self.tokens, self.label.clone(), self.shortcut.clone())
    }
}

pub fn tooltip_view(
    tokens: ThemeTokens,
    label: impl Into<String>,
    shortcut: Option<String>,
    cx: &mut App,
) -> AnyView {
    // GPUI mounts native tooltips at the window layer, keeping them outside
    // scroll masks and other clipped feature subtrees.
    cx.new(|_| TooltipView {
        tokens,
        label: label.into(),
        shortcut,
    })
    .into()
}

pub fn tooltip_content(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    shortcut: Option<String>,
) -> AnyElement {
    let label = label.into();
    let animation_id = gpui::ElementId::Name(format!("tooltip-enter-{label}").into());
    let tooltip = tooltip_surface(tokens, label, shortcut);
    crate::motion::fade_in(
        tokens,
        animation_id,
        tooltip,
        crate::motion::MotionDuration::Micro,
    )
}

/// Unanimated surface for owners that retain tooltips through an exit transition.
pub fn tooltip_surface(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    shortcut: Option<String>,
) -> Div {
    let label = label.into();
    crate::surface::material_surface(tokens, div(), crate::surface::MaterialRole::Popover)
        .rounded(px(tokens.radii.xs))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .px(px(tokens.metrics.ui_tooltip_padding_x))
        .py(px(tokens.metrics.ui_tooltip_padding_y))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text))
        .shadow_lg()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .child(label)
                .when_some(shortcut, |row, shortcut| {
                    row.child(
                        div()
                            .ml_auto()
                            .rounded(px(tokens.radii.xs))
                            .border_1()
                            .border_color(rgb(tokens.ui.border))
                            .bg(rgb(tokens.ui.bg))
                            .px(px(tokens.spacing.one))
                            .py(px(tokens.spacing.one / 2.0))
                            .text_size(px(tokens.metrics.ui_tooltip_shortcut_font_size))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(shortcut),
                    )
                }),
        )
}
