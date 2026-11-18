use gpui::{AnyElement, Div, MouseButton, div, prelude::*};

use oxideterm_theme::ThemeTokens;

/// Keeps a settings dialog mounted during its exit animation while an inert
/// overlay prevents the retained form payload from receiving more input.
pub fn settings_dialog_transition(
    tokens: &ThemeTokens,
    animation_id: &'static str,
    mut backdrop: Div,
    form: Div,
    phase: oxideterm_gpui_ui::motion::ExitPhase,
) -> AnyElement {
    let is_visible = phase == oxideterm_gpui_ui::motion::ExitPhase::Visible;
    // Strip any static scrim from caller backdrop builders so scrim dim and
    // foreground dialog scale/fade animate on independent tracks.
    backdrop.style().background = None;
    backdrop.style().backdrop_filter = None;
    backdrop
        .child(oxideterm_gpui_ui::modal::staged_overlay_scrim(
            tokens,
            format!("{animation_id}-scrim"),
            oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
            is_visible,
        ))
        .child(oxideterm_gpui_ui::motion::form_transition(
            tokens,
            animation_id,
            form,
            is_visible,
        ))
        .when(!is_visible, settings_dialog_inert_overlay)
        .into_any_element()
}

pub fn settings_dialog_inert_overlay(backdrop: Div) -> Div {
    backdrop.child(
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_scroll(|_event, _window, cx| cx.stop_propagation()),
    )
}
