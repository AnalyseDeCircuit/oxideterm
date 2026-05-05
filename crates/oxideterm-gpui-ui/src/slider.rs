use gpui::{Div, ParentElement, Styled, div, px, relative, rgb, rgba};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug)]
pub struct SliderView {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub disabled: bool,
}

impl SliderView {
    pub fn percent(self) -> f32 {
        if (self.max - self.min).abs() <= f32::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }
}

pub fn slider(tokens: &ThemeTokens, view: SliderView) -> Div {
    let pct = view.percent();
    let thumb = tokens.metrics.ui_slider_thumb_size;
    div()
        .relative()
        .flex()
        .items_center()
        .opacity(if view.disabled { 0.5 } else { 1.0 })
        .child(
            div()
                .relative()
                .h(px(tokens.metrics.ui_slider_track_height))
                .w_full()
                .rounded_full()
                .bg(rgba((tokens.ui.border << 8) | 0x99))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .rounded_full()
                        .bg(rgb(tokens.ui.accent))
                        .w(relative(pct)),
                ),
        )
        .child(
            div()
                .absolute()
                .size(px(thumb))
                .rounded_full()
                .border_1()
                .border_color(rgba(0x00000033))
                .bg(rgb(0xffffff))
                .left(relative(pct))
                .ml(px(-thumb / 2.0)),
        )
}
