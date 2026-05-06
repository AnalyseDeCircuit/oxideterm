use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Bounds, Div, Element, ElementId, GlobalElementId, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, ParentElement, Pixels, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SelectAnchorId {
    SettingsLanguage,
    SettingsAppearanceTheme,
    SettingsAppearanceDensity,
    SettingsAppearanceBorderRadiusSlider,
    SettingsAppearanceAnimation,
    SettingsAppearanceRenderProfile,
    SettingsAppearanceFrostedGlass,
    SettingsAppearanceBackgroundOpacitySlider,
    SettingsAppearanceBackgroundBlurSlider,
    SettingsAppearanceBackgroundFit,
    SettingsTerminalFontFamily,
    SettingsTerminalFontSizeSlider,
    SettingsTerminalEncoding,
    SettingsTerminalAdaptiveRenderer,
    SettingsTerminalCursorStyle,
    SettingsLocalShell,
    SettingsConnectionIdleTimeout,
    SettingsHighlightPreset,
    SettingsHighlightRenderMode(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayAnchor {
    pub id: SelectAnchorId,
    pub bounds: Bounds<Pixels>,
}

type AnchorBoundsCallback = Box<dyn FnOnce(OverlayAnchor, &mut Window, &mut App)>;

pub struct SelectAnchorProbe {
    id: SelectAnchorId,
    child: Option<AnyElement>,
    on_bounds: Option<AnchorBoundsCallback>,
}

pub fn select_anchor_probe(
    id: SelectAnchorId,
    child: impl IntoElement,
    on_bounds: impl FnOnce(OverlayAnchor, &mut Window, &mut App) + 'static,
) -> SelectAnchorProbe {
    SelectAnchorProbe {
        id,
        child: Some(child.into_any_element()),
        on_bounds: Some(Box::new(on_bounds)),
    }
}

impl IntoElement for SelectAnchorProbe {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectAnchorProbe {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = self
            .child
            .as_mut()
            .expect("select anchor child should render once")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        if let Some(on_bounds) = self.on_bounds.take() {
            let anchor = OverlayAnchor {
                id: self.id,
                bounds,
            };
            window.on_next_frame(move |window, cx| on_bounds(anchor, window, cx));
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

pub fn select_trigger(
    tokens: &ThemeTokens,
    value: impl Into<String>,
    placeholder: bool,
    disabled: bool,
) -> Div {
    div()
        .h(px(tokens.metrics.ui_control_height))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | 0x80))
        .bg(rgba((tokens.ui.bg << 8) | 0x80))
        .px(px(tokens.metrics.ui_control_padding_x))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(if placeholder {
            tokens.ui.text_muted
        } else {
            tokens.ui.text
        }))
        .opacity(if disabled { 0.5 } else { 1.0 })
        .child(div().flex_1().min_w(px(0.0)).truncate().child(value.into()))
        .child(
            div()
                .ml(px(tokens.spacing.two))
                .text_color(rgb(tokens.ui.text_muted))
                .opacity(0.5)
                .child("⌄"),
        )
}

pub fn select_popup(tokens: &ThemeTokens, width: f32) -> Stateful<Div> {
    select_popup_with_max_height(tokens, width, tokens.metrics.ui_select_max_height)
}

pub fn select_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    div()
        .id("select-popup-scroll")
        .w(px(width.max(tokens.metrics.ui_select_min_width)))
        .max_h(px(max_height))
        .overflow_y_scroll()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(elevated_background(tokens))
        .p(px(tokens.metrics.ui_menu_padding))
        .text_color(rgb(tokens.ui.text))
        .shadow_lg()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn select_panel_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    select_popup_with_max_height(tokens, width, max_height).bg(rgb(tokens.ui.bg_panel))
}

pub fn select_overlay_popup(tokens: &ThemeTokens, width: f32) -> Stateful<Div> {
    select_popup(tokens, width)
}

pub fn select_overlay_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    select_popup_with_max_height(tokens, width, max_height)
}

pub fn select_panel_overlay_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    select_panel_popup_with_max_height(tokens, width, max_height)
}

pub fn select_option(tokens: &ThemeTokens, label: impl Into<String>, selected: bool) -> Div {
    select_item(tokens, label, selected)
        .cursor_pointer()
        .hover(|item| item.bg(rgb(tokens.ui.bg_hover)))
}

pub fn select_content(tokens: &ThemeTokens) -> Div {
    div()
        .relative()
        .max_h(px(tokens.metrics.ui_select_max_height))
        .min_w(px(tokens.metrics.ui_select_min_width))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_elevated << 8) | 0xf2))
        .text_color(rgb(tokens.ui.text))
        .shadow_lg()
        .child(div().p(px(tokens.metrics.ui_menu_padding)))
}

pub fn select_item(tokens: &ThemeTokens, label: impl Into<String>, selected: bool) -> Div {
    div()
        .relative()
        .flex()
        .w_full()
        .items_center()
        .rounded(px(tokens.radii.xs))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .pl(px(tokens.metrics.ui_menu_item_padding_x))
        .pr(px(tokens.metrics.ui_menu_inset_padding_left))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .when(selected, |item| {
            item.bg(rgba((tokens.ui.bg_hover << 8) | 0x80))
        })
        .child(
            div()
                .absolute()
                .right(px(tokens.metrics.ui_menu_item_padding_x))
                .size(px(tokens.metrics.ui_select_check_size))
                .flex()
                .items_center()
                .justify_center()
                .child(if selected { "✓" } else { "" }),
        )
        .child(label.into())
}

pub fn select_label(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    let label = label.into().to_uppercase();
    div()
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(label)
}

pub fn select_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(-tokens.metrics.ui_menu_padding))
        .my(px(tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(rgb(tokens.ui.border))
}

fn elevated_background(tokens: &ThemeTokens) -> gpui::Rgba {
    rgba((tokens.ui.bg_elevated << 8) | 0xf2)
}
