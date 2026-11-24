use gpui::{AnyElement, MouseButton, div, prelude::*, px};
use oxideterm_gpui_ui::{
    select::{SelectAnchorId, select_anchor_probe},
    slider::{SliderView, slider},
};
use oxideterm_settings::{MAX_WINDOW_OPACITY, MIN_WINDOW_OPACITY};
use oxideterm_theme::ThemeTokens;

use crate::{
    SettingsSlider,
    components::{SettingsWindowDispatch, SettingsWindowRequest},
};

pub fn render_appearance_slider(
    tokens: &ThemeTokens,
    id: SettingsSlider,
    anchor: SelectAnchorId,
    min: f32,
    max: f32,
    value: f32,
    dispatch: SettingsWindowDispatch,
) -> AnyElement {
    let down = dispatch.clone();
    let up = dispatch.clone();
    let motion = dispatch.clone();
    div()
        .w(px(tokens.metrics.settings_slider_width))
        .child(select_anchor_probe(
            anchor,
            slider(
                tokens,
                SliderView {
                    min,
                    max,
                    value,
                    disabled: false,
                },
            )
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                down(
                    SettingsWindowRequest::BeginSlider(id, f32::from(event.position.x)),
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                up(SettingsWindowRequest::FinishSlider, window, cx);
                cx.stop_propagation();
            })
            .on_mouse_move(move |event, window, cx| {
                motion(SettingsWindowRequest::MoveSlider(event.clone()), window, cx)
            }),
            move |anchor, window, cx| {
                dispatch(SettingsWindowRequest::SliderAnchor(anchor), window, cx)
            },
        ))
        .into_any_element()
}

// Persisted opacity keeps 0.1% steps so the slider's expanded near-opaque
// range is not collapsed back into whole-percent jumps.
const WINDOW_OPACITY_STEPS_PER_UNIT: f64 = 1000.0;

/// Maps window opacity to slider travel in `0.0..=1.0`.
///
/// Compositors blend window alpha linearly, but on a dark theme over bright
/// content the first few percent of transparency are already very visible.
/// Travel is quadratic in transparency so most of the slider covers the
/// near-opaque range.
pub fn window_opacity_slider_position(opacity: f64) -> f32 {
    let transparency = ((MAX_WINDOW_OPACITY - opacity) / (MAX_WINDOW_OPACITY - MIN_WINDOW_OPACITY))
        .clamp(0.0, 1.0);
    (1.0 - transparency.sqrt()) as f32
}

pub fn window_opacity_from_slider_position(position: f32) -> f64 {
    let travel_from_opaque = 1.0 - f64::from(position.clamp(0.0, 1.0));
    let opacity =
        MAX_WINDOW_OPACITY - (MAX_WINDOW_OPACITY - MIN_WINDOW_OPACITY) * travel_from_opaque.powi(2);
    (opacity * WINDOW_OPACITY_STEPS_PER_UNIT).round() / WINDOW_OPACITY_STEPS_PER_UNIT
}
